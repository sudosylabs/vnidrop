#[cfg(unix)]
use std::os::fd::{FromRawFd, OwnedFd};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{Context, Result};
use bytes::Bytes;
use iroh_blobs::{api::TempTag, Hash};
use uuid::Uuid;

use crate::{
    api::{CoreLimits, ShareSource, SourceKind},
    util::non_empty,
};

const STREAM_BUFFER_LEN: usize = 1024 * 1024;
const STALE_PART_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug)]
pub(crate) struct TransferImport {
    pub(crate) tag: TempTag,
    pub(crate) root_hash: Hash,
    /// Per-file content hashes in the collection. Provider ACL maps these too
    /// so child blob gets are not fail-open when only the root is tracked.
    pub(crate) member_hashes: Vec<Hash>,
    pub(crate) total_size: u64,
    pub(crate) file_count: u64,
    pub(crate) default_name: String,
}

#[derive(Debug)]
pub(crate) struct ImportSourceFile {
    pub(crate) source: ImportSource,
    pub(crate) collection_name: String,
}

#[derive(Debug)]
pub(crate) enum ImportSource {
    Path(PathBuf),
    #[cfg(unix)]
    FileDescriptor(OwnedFd),
}

pub(crate) struct AtomicOutputFile {
    target: PathBuf,
    temporary: PathBuf,
    committed: bool,
}

impl AtomicOutputFile {
    pub(crate) fn create(output_dir: &Path, relative_path: &str) -> Result<(Self, File)> {
        let target = safe_output_path(output_dir, relative_path)?;
        let parent = target
            .parent()
            .context("output file must have a parent directory")?;
        std::fs::create_dir_all(parent)?;

        let canonical_root = std::fs::canonicalize(output_dir)?;
        let canonical_parent = std::fs::canonicalize(parent)?;
        if !canonical_parent.starts_with(&canonical_root) {
            anyhow::bail!("output path escapes the selected directory");
        }
        cleanup_stale_temporary_files(parent, STALE_PART_AGE)?;
        if std::fs::symlink_metadata(&target).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("destination already exists: {}", target.display()),
            )
            .into());
        }

        let final_name = target
            .file_name()
            .and_then(|name| name.to_str())
            .context("output filename is not valid UTF-8")?;
        let temporary = parent.join(format!(".{final_name}.vnidrop-{}.part", Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);
        let file = options
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;

        // Recheck after opening the temporary file so a swapped ancestor is
        // detected before bytes are published to the final destination.
        let canonical_parent_after_open = std::fs::canonicalize(parent)?;
        if canonical_parent_after_open != canonical_parent {
            let _ = std::fs::remove_file(&temporary);
            anyhow::bail!("output directory changed while opening destination");
        }

        Ok((
            Self {
                target,
                temporary,
                committed: false,
            },
            file,
        ))
    }

    pub(crate) fn commit(mut self) -> Result<()> {
        publish_temp_as_final(&self.temporary, &self.target)
            .with_context(|| format!("failed to commit {}", self.target.display()))?;
        self.committed = true;
        Ok(())
    }

    pub(crate) fn target(&self) -> &Path {
        &self.target
    }
}

/// Publish a fully written temporary file as the final destination without
/// clobbering an existing peer.
///
/// Prefer a same-directory hard link (atomic no-clobber on most Unix volumes).
/// Android's emulated external storage and some FUSE mounts reject hard links
/// even when ordinary create/write/rename work, so fall back to an exclusive
/// rename when the link is unsupported.
fn publish_temp_as_final(temporary: &Path, target: &Path) -> io::Result<()> {
    match std::fs::hard_link(temporary, target) {
        Ok(()) => {
            if let Err(error) = std::fs::remove_file(temporary) {
                tracing::warn!(
                    %error,
                    path = %temporary.display(),
                    "failed to remove committed temporary file"
                );
            }
            Ok(())
        }
        Err(error) if is_hard_link_unsupported(&error) => rename_no_replace(temporary, target),
        Err(error) => Err(error),
    }
}

fn is_hard_link_unsupported(error: &io::Error) -> bool {
    match error.raw_os_error() {
        Some(code)
            if code == libc::EPERM
                || code == libc::EACCES
                || code == libc::EOPNOTSUPP
                || code == libc::ENOTSUP
                || code == libc::EXDEV
                || code == libc::EINVAL
                || code == libc::ENOSYS =>
        {
            true
        }
        _ => matches!(
            error.kind(),
            io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied
        ),
    }
}

fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        match renameat2_noreplace(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if error.raw_os_error() == Some(libc::ENOSYS)
                    || error.raw_os_error() == Some(libc::EINVAL) => {}
            Err(error) => return Err(error),
        }
    }

    #[cfg(target_os = "macos")]
    {
        match renamex_np_excl(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if error.raw_os_error() == Some(libc::ENOTSUP) => {}
            Err(error) => return Err(error),
        }
    }

    // Last resort: refuse an existing destination, then rename. There is a
    // small race versus concurrent writers, but this path only runs when the
    // platform lacks both hard links and exclusive rename.
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("destination already exists: {}", to.display()),
        ));
    }
    std::fs::rename(from, to)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn renameat2_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let from_c = CString::new(from.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let to_c = CString::new(to.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    // Android defines RENAME_NOREPLACE as c_int while renameat2 takes c_uint.
    let flags = libc::RENAME_NOREPLACE as libc::c_uint;
    let rc = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from_c.as_ptr(),
            libc::AT_FDCWD,
            to_c.as_ptr(),
            flags,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn renamex_np_excl(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let from_c = CString::new(from.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let to_c = CString::new(to.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let rc = unsafe { libc::renamex_np(from_c.as_ptr(), to_c.as_ptr(), libc::RENAME_EXCL) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(crate) fn cleanup_stale_temporary_files(
    directory: &Path,
    minimum_age: Duration,
) -> Result<u64> {
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with('.') && name.contains(".vnidrop-") && name.ends_with(".part")) {
            continue;
        }
        let metadata = entry.metadata()?;
        let age = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or_default();
        if age >= minimum_age && metadata.is_file() {
            std::fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

impl Drop for AtomicOutputFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.temporary);
        }
    }
}

impl ImportSource {
    pub(crate) fn open(self) -> Result<File> {
        match self {
            Self::Path(path) => {
                File::open(&path).with_context(|| format!("failed to open {}", path.display()))
            }
            #[cfg(unix)]
            Self::FileDescriptor(fd) => Ok(File::from(fd)),
        }
    }
}

#[cfg(test)]
pub(crate) fn collect_import_files(sources: Vec<ShareSource>) -> Result<Vec<ImportSourceFile>> {
    collect_import_files_with_limits(sources, &CoreLimits::default())
}

pub(crate) fn collect_import_files_with_limits(
    sources: Vec<ShareSource>,
    limits: &CoreLimits,
) -> Result<Vec<ImportSourceFile>> {
    if sources.len() as u64 > limits.max_sources {
        anyhow::bail!(
            "source count {} exceeds limit {}",
            sources.len(),
            limits.max_sources
        );
    }
    let mut files = Vec::new();
    for source in sources {
        match source.kind {
            SourceKind::Path | SourceKind::IosSecurityScopedUrl => {
                // iOS security-scoped resources are still ordinary paths once
                // the platform side has started the lease.  Rust deliberately
                // does not try to own that lease; it only streams while Kotlin
                // keeps the URL accessible.
                let path = source_path(&source)?;
                let display_name = source
                    .display_name
                    .clone()
                    .and_then(non_empty)
                    .or_else(|| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .map(ToOwned::to_owned)
                    })
                    .unwrap_or_else(|| "transfer".to_string());
                if source.is_directory || path.is_dir() {
                    collect_dir_files(&path, &display_name, &mut files)?;
                } else {
                    files.push(ImportSourceFile {
                        source: ImportSource::Path(path),
                        collection_name: validated_relative_string(&display_name)?,
                    });
                }
            }
            SourceKind::FileDescriptor => {
                #[cfg(not(unix))]
                {
                    anyhow::bail!(
                        "file descriptor sources are only supported on Unix-like targets"
                    );
                }
                #[cfg(unix)]
                {
                    if source.is_directory {
                        anyhow::bail!("file descriptor sources cannot represent directories yet");
                    }
                    let display_name = source
                        .display_name
                        .clone()
                        .and_then(non_empty)
                        .unwrap_or_else(|| "transfer".to_string());
                    // Android SAF/content URIs are not paths.  Platform code opens
                    // the URI as a ParcelFileDescriptor, then Rust duplicates the
                    // borrowed fd here and streams from its owned duplicate.
                    files.push(ImportSourceFile {
                        source: ImportSource::FileDescriptor(duplicate_file_descriptor(
                            &source.value,
                        )?),
                        collection_name: validated_relative_string(&display_name)?,
                    });
                }
            }
            SourceKind::AndroidContentUri => {
                anyhow::bail!(
                    "Android content URIs are not filesystem paths; open a ParcelFileDescriptor in Kotlin and pass its fd as SourceKind.FileDescriptor"
                );
            }
        }
    }
    if files.is_empty() {
        anyhow::bail!("no files found in selected sources");
    }
    if files.len() as u64 > limits.max_collection_files {
        anyhow::bail!(
            "collection file count {} exceeds limit {}",
            files.len(),
            limits.max_collection_files
        );
    }
    let mut known_total = 0u64;
    for file in &files {
        if file.collection_name.len() as u64 > limits.max_path_bytes {
            anyhow::bail!(
                "collection path exceeds {} bytes: {}",
                limits.max_path_bytes,
                file.collection_name
            );
        }
        #[cfg(not(unix))]
        let ImportSource::Path(path) = &file.source;
        #[cfg(unix)]
        let path = match &file.source {
            ImportSource::Path(path) => path,
            ImportSource::FileDescriptor(_) => continue,
        };
        known_total = known_total
            .checked_add(std::fs::metadata(path)?.len())
            .context("collection size overflow")?;
        if known_total > limits.max_total_bytes {
            anyhow::bail!(
                "collection size {known_total} exceeds limit {}",
                limits.max_total_bytes
            );
        }
    }
    Ok(files)
}

fn source_path(source: &ShareSource) -> Result<PathBuf> {
    if source.value.trim().is_empty() {
        anyhow::bail!("source path must not be empty");
    }
    platform_path(&source.value)
}

pub(crate) fn platform_path(value: &str) -> Result<PathBuf> {
    if value.trim().is_empty() {
        anyhow::bail!("path must not be empty");
    }
    if let Some(without_scheme) = value.strip_prefix("file://") {
        return Ok(PathBuf::from(percent_decode_file_url_path(without_scheme)?));
    }
    Ok(PathBuf::from(value))
}

fn collect_dir_files(
    root: &Path,
    display_name: &str,
    files: &mut Vec<ImportSourceFile>,
) -> Result<()> {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .context("failed to compute relative path")?;
        let collection_name = path_to_string(Path::new(display_name).join(relative), true)?;
        files.push(ImportSourceFile {
            source: ImportSource::Path(entry.path().to_path_buf()),
            collection_name,
        });
    }
    Ok(())
}

pub(crate) fn default_collection_name(files: &[ImportSourceFile]) -> String {
    files
        .first()
        .and_then(|file| file.collection_name.split('/').next())
        .filter(|name| !name.is_empty())
        .unwrap_or("transfer")
        .to_string()
}

pub(crate) fn safe_output_path(output_dir: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = Path::new(relative_path);
    path_to_string(relative, true)?;
    Ok(output_dir.join(relative))
}

pub(crate) fn read_stream_from_blocking_reader<R>(
    mut reader: R,
) -> impl futures::Stream<Item = io::Result<Bytes>> + Send + Sync + 'static
where
    R: Read + Send + 'static,
{
    // Keep the FFI boundary out of the data path: blocking OS reads feed a
    // small bounded channel, so large files are streamed into iroh-blobs
    // without copying the whole file through Kotlin memory.
    let (tx, rx) = async_channel::bounded(2);
    std::thread::spawn(move || {
        let mut buffer = vec![0; STREAM_BUFFER_LEN];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if tx
                        .send_blocking(Ok(Bytes::copy_from_slice(&buffer[..read])))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    let _ = tx.send_blocking(Err(error));
                    break;
                }
            }
        }
    });
    rx
}

pub(crate) fn write_stream_to_blocking_writer<W>(
    mut writer: W,
    rx: async_channel::Receiver<io::Result<Option<Bytes>>>,
) -> io::Result<W>
where
    W: Write,
{
    while let Ok(item) = rx.recv_blocking() {
        match item? {
            Some(bytes) => writer.write_all(&bytes)?,
            None => break,
        }
    }
    writer.flush()?;
    Ok(writer)
}

pub(crate) async fn wait_for_writer<T: Send + 'static>(
    task: std::thread::JoinHandle<io::Result<T>>,
) -> Result<io::Result<T>> {
    tokio::task::spawn_blocking(move || {
        task.join()
            .map_err(|_| anyhow::anyhow!("export writer thread panicked"))
    })
    .await?
}

pub(crate) fn validated_relative_string(name: &str) -> Result<String> {
    let value = path_to_string(Path::new(name), true)?;
    if value.trim().is_empty() {
        anyhow::bail!("relative path must not be empty");
    }
    Ok(value)
}

pub(crate) fn path_to_string(path: impl AsRef<Path>, must_be_relative: bool) -> Result<String> {
    let mut path_str = String::new();
    let parts = path
        .as_ref()
        .components()
        .filter_map(|component| match component {
            Component::Normal(x) => {
                let Some(component) = x.to_str() else {
                    return Some(Err(anyhow::anyhow!("invalid character in path")));
                };
                if !component.contains('/') && !component.contains('\\') {
                    Some(Ok(component))
                } else {
                    Some(Err(anyhow::anyhow!("invalid path component {component:?}")))
                }
            }
            Component::RootDir => {
                if must_be_relative {
                    Some(Err(anyhow::anyhow!("invalid root path component")))
                } else {
                    path_str.push('/');
                    None
                }
            }
            other => Some(Err(anyhow::anyhow!("invalid path component {other:?}"))),
        })
        .collect::<Result<Vec<_>>>()?;
    path_str.push_str(&parts.join("/"));
    Ok(path_str)
}

pub(crate) fn percent_decode_file_url_path(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                anyhow::bail!("invalid percent escape in file URL");
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])?;
            output.push(u8::from_str_radix(hex, 16).context("invalid percent escape in file URL")?);
            i += 3;
        } else {
            output.push(bytes[i]);
            i += 1;
        }
    }
    Ok(String::from_utf8(output)?)
}

#[cfg(unix)]
fn duplicate_file_descriptor(value: &str) -> Result<OwnedFd> {
    let fd = value
        .parse::<i32>()
        .context("file descriptor source value must be an integer fd")?;
    if fd < 0 {
        anyhow::bail!("file descriptor must be non-negative");
    }

    // Android's ParcelFileDescriptor remains owned by Kotlin.  Rust duplicates
    // it immediately so the blocking import thread can close its own handle
    // without racing or invalidating the platform owner.
    let duplicated = unsafe { libc::dup(fd) };
    if duplicated < 0 {
        return Err(io::Error::last_os_error()).context("failed to duplicate file descriptor");
    }

    // Only regular files are valid share sources. Reject directories, sockets,
    // and other fd types that a compromised UI could pass by integer.
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let rc = unsafe { libc::fstat(duplicated, stat.as_mut_ptr()) };
    if rc != 0 {
        let error = io::Error::last_os_error();
        unsafe {
            libc::close(duplicated);
        }
        return Err(error).context("failed to fstat file descriptor");
    }
    let mode = unsafe { stat.assume_init() }.st_mode;
    if mode & libc::S_IFMT != libc::S_IFREG {
        unsafe {
            libc::close(duplicated);
        }
        anyhow::bail!("file descriptor must refer to a regular file");
    }
    Ok(unsafe { OwnedFd::from_raw_fd(duplicated) })
}
