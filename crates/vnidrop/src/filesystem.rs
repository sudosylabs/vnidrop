#[cfg(unix)]
use std::os::fd::{FromRawFd, OwnedFd};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result};
use bytes::Bytes;
use iroh_blobs::{api::TempTag, Hash};

use crate::{
    api::{CoreLimits, ShareSource, SourceKind},
    output_staging::OutputStaging,
    util::non_empty,
};

const STREAM_BUFFER_LEN: usize = 1024 * 1024;

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
    canonical_target: PathBuf,
    staging: OutputStaging,
}

impl AtomicOutputFile {
    #[cfg(test)]
    pub(crate) fn create(output_dir: &Path, relative_path: &str) -> Result<(Self, File)> {
        Self::create_tracked(output_dir, relative_path, None)
    }

    pub(crate) fn create_tracked(
        output_dir: &Path,
        relative_path: &str,
        profile: Option<&Path>,
    ) -> Result<(Self, File)> {
        let target = safe_output_path(output_dir, relative_path)?;
        let parent = target
            .parent()
            .context("output file must have a parent directory")?;
        let canonical_root = std::fs::canonicalize(output_dir)?;
        let relative_parent = parent
            .strip_prefix(output_dir)
            .context("output parent is outside its root")?;
        let mut canonical_parent = canonical_root.clone();
        for component in relative_parent.components() {
            if let Component::Normal(name) = component {
                canonical_parent.push(name);
                match std::fs::create_dir(&canonical_parent) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                let metadata = std::fs::symlink_metadata(&canonical_parent)?;
                if !metadata.is_dir() || is_reparse_directory(&metadata) {
                    anyhow::bail!("output path contains a linked directory");
                }
                canonical_parent = std::fs::canonicalize(&canonical_parent)?;
                if !canonical_parent.starts_with(&canonical_root) {
                    anyhow::bail!("output path escapes the selected directory");
                }
            }
        }
        if std::fs::symlink_metadata(&target).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("destination already exists: {}", target.display()),
            )
            .into());
        }

        let canonical_target =
            canonical_parent.join(target.file_name().context("missing output filename")?);
        let (staging, file) = OutputStaging::create(&canonical_parent, profile)?;
        if std::fs::canonicalize(parent)? != canonical_parent {
            drop(file);
            anyhow::bail!("output directory changed while opening destination");
        }
        Ok((
            Self {
                target,
                canonical_target,
                staging,
            },
            file,
        ))
    }

    pub(crate) fn commit(self) -> Result<()> {
        let parent = self
            .canonical_target
            .parent()
            .context("missing output parent")?;
        if std::fs::canonicalize(parent)? != parent {
            anyhow::bail!("output directory changed before publication");
        }
        publish_temp_as_final(&self.staging.path, &self.canonical_target)
            .with_context(|| format!("failed to commit {}", self.target.display()))
    }

    pub(crate) fn target(&self) -> &Path {
        &self.target
    }
}

#[cfg(windows)]
fn is_reparse_directory(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_directory(_: &std::fs::Metadata) -> bool {
    false
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

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    renameat2_noreplace(from, to)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    renamex_np_excl(from, to)
}

#[cfg(windows)]
pub(crate) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    let from = from
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let to = to.as_os_str().encode_wide().chain([0]).collect::<Vec<_>>();
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    windows
)))]
pub(crate) fn rename_no_replace(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "exclusive publication is unavailable",
    ))
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

#[cfg(any(target_os = "macos", target_os = "ios"))]
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

#[cfg(unix)]
pub(crate) fn output_file_identity(file: &File) -> io::Result<String> {
    use std::os::unix::fs::MetadataExt;

    let metadata = file.metadata()?;
    Ok(format!("{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
pub(crate) fn output_file_identity(file: &File) -> io::Result<String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(format!(
        "{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

pub(crate) fn verify_published_output(path: &Path, identity: &str, hash: Hash) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    }
    let file = File::open(path)?;
    let actual_identity = output_file_identity(&file)?;
    if actual_identity != identity
        && !moved_output_on_same_volume(&file, &actual_identity, identity)
    {
        return Ok(false);
    }
    verify_file_contents(file, hash)
}

#[cfg(windows)]
fn moved_output_on_same_volume(file: &File, actual: &str, checkpoint: &str) -> bool {
    use std::{os::windows::io::AsRawHandle, ptr::null_mut};
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationByHandleW;

    if actual
        .split_once(':')
        .zip(checkpoint.split_once(':'))
        .is_none_or(|((actual_volume, _), (expected_volume, _))| actual_volume != expected_volume)
    {
        return false;
    }

    // FAT indexes can change when a file moves out of staging.
    let mut name = [0u16; 16];
    let result = unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    };
    let length = name
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(name.len());
    result != 0
        && matches!(
            String::from_utf16_lossy(&name[..length])
                .to_ascii_lowercase()
                .as_str(),
            "fat" | "fat32" | "exfat"
        )
}

#[cfg(not(windows))]
fn moved_output_on_same_volume(_: &File, _: &str, _: &str) -> bool {
    false
}

fn verify_file_contents(file: File, hash: Hash) -> io::Result<bool> {
    if !file.metadata()?.is_file() {
        return Ok(false);
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(file)?;
    Ok(hasher.finalize().as_bytes() == hash.as_bytes())
}

#[cfg(unix)]
pub(crate) fn verify_owned_descriptor(
    descriptor: i32,
    hash: Hash,
    size: u64,
    timeout: std::time::Duration,
) -> io::Result<bool> {
    use std::os::unix::fs::FileTypeExt;

    if descriptor < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid output descriptor",
        ));
    }
    // The foreign verifier transfers ownership of the valid descriptor.
    let file = unsafe { File::from_raw_fd(descriptor) };
    let kind = file.metadata()?.file_type();
    let limit = size.checked_add(1).ok_or(io::ErrorKind::InvalidData)?;
    let mut input = if kind.is_file() {
        (Box::new(file) as Box<dyn Read>).take(limit)
    } else if kind.is_fifo() || kind.is_socket() {
        (Box::new(TimedDescriptor::new(file, timeout)?) as Box<dyn Read>).take(limit)
    } else {
        return Ok(false);
    };
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(&mut input)?;
    Ok(limit - input.limit() == size && hasher.finalize().as_bytes() == hash.as_bytes())
}

#[cfg(unix)]
struct TimedDescriptor {
    file: File,
    timeout: std::time::Duration,
}

#[cfg(unix)]
impl TimedDescriptor {
    fn new(file: File, timeout: std::time::Duration) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let fd = file.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { file, timeout })
    }
}

#[cfg(unix)]
impl Read for TimedDescriptor {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        use std::{os::fd::AsRawFd, time::Instant};

        let started = Instant::now();
        loop {
            match self.file.read(buffer) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                result => return result,
            }
            let remaining = self.timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let mut descriptor = libc::pollfd {
                fd: self.file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let wait_ms = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
            let ready = unsafe { libc::poll(&mut descriptor, 1, wait_ms) };
            if ready < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(io::Error::last_os_error());
            }
        }
    }
}

#[cfg(not(unix))]
pub(crate) fn verify_owned_descriptor(
    _: i32,
    _: Hash,
    _: u64,
    _: std::time::Duration,
) -> io::Result<bool> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "output descriptors are unavailable",
    ))
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
