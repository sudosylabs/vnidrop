use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use uuid::Uuid;

const JOURNAL: &str = "receive-staging";
const MARKER: &str = "owner";
const PAYLOAD: &str = "payload";

pub(crate) struct OutputStaging {
    pub(crate) path: PathBuf,
    record: Option<LockedRecord>,
}

impl OutputStaging {
    pub(crate) fn create(parent: &Path, profile: Option<&Path>) -> Result<(Self, File)> {
        let id = Uuid::new_v4();
        let Some(profile) = profile else {
            let path = parent.join(format!(".vnidrop-{id}.part"));
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            return Ok((Self { path, record: None }, file));
        };
        let journal = profile.join(JOURNAL);
        fs::create_dir_all(&journal)?;
        let record_path = journal.join(id.to_string());
        let pending_path = record_path.with_extension("pending");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&pending_path)?;
        lock_record(&file)?;
        let mut record = LockedRecord {
            path: pending_path,
            file,
        };
        let nonce = Uuid::new_v4().to_string();
        let directory = parent.join(format!(".vnidrop-{id}"));
        if let Err(error) = create_staging_directory(&directory) {
            record.remove();
            return Err(error.into());
        }
        let path = directory.join(PAYLOAD);
        let mut staging = Self { path, record: None };
        let result = (|| -> Result<File> {
            fs::write(directory.join(MARKER), &nonce)?;
            serde_json::to_writer(&mut record.file, &(&directory, &nonce))?;
            record.file.flush()?;
            record.file.sync_all()?;
            fs::rename(&record.path, &record_path)?;
            record.path = record_path;
            Ok(OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging.path)?)
        })();
        staging.record = Some(record);
        match result {
            Ok(file) => Ok((staging, file)),
            Err(error) => Err(error),
        }
    }
}

impl Drop for OutputStaging {
    fn drop(&mut self) {
        if remove_if_present(&self.path).is_err() {
            return;
        }
        if let Some(record) = self.record.take() {
            let directory = self.path.parent().expect("staging directory");
            if remove_if_present(&directory.join(MARKER)).is_ok() {
                let _ = fs::remove_dir(directory);
                record.remove();
            }
        }
    }
}

struct LockedRecord {
    path: PathBuf,
    file: File,
}

impl LockedRecord {
    fn remove(self) {
        let Self { path, file } = self;
        drop(file);
        let _ = fs::remove_file(path);
    }
}

pub(crate) fn inspect_or_reclaim(profile: &Path, reclaim: bool) -> Result<u64> {
    let journal = match fs::read_dir(profile.join(JOURNAL)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    let mut total = 0u64;
    for entry in journal {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let pending = name.ends_with(".pending");
        let Ok(id) = Uuid::parse_str(name.strip_suffix(".pending").unwrap_or(name)) else {
            continue;
        };
        if pending
            && (!reclaim
                || entry
                    .metadata()?
                    .modified()
                    .ok()
                    .and_then(|mtime| SystemTime::now().duration_since(mtime).ok())
                    .is_none_or(|age| age < Duration::from_secs(3600)))
        {
            continue;
        }
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) else {
            continue;
        };
        if lock_record(&file).is_err() {
            continue;
        }
        let mut record = LockedRecord { path, file };
        let parsed =
            serde_json::from_reader::<_, (PathBuf, String)>((&mut record.file).take(65_536));
        let Ok((directory, nonce)) = parsed else {
            if reclaim {
                record.remove();
            }
            continue;
        };
        if !directory.is_absolute()
            || directory.file_name().and_then(|name| name.to_str())
                != Some(&format!(".vnidrop-{id}"))
        {
            continue;
        }
        if !fs::symlink_metadata(&directory).is_ok_and(|metadata| metadata.is_dir())
            || !marker_matches(&directory, &nonce)
        {
            // Keep ownership metadata when the destination is unavailable.
            continue;
        }

        let payload = directory.join(PAYLOAD);
        match fs::symlink_metadata(&payload) {
            Ok(metadata) if metadata.is_file() => {
                if reclaim {
                    fs::remove_file(&payload).context("failed to remove staged receive payload")?;
                }
                total = total.saturating_add(metadata.len());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            _ => continue,
        }
        if reclaim {
            remove_if_present(&directory.join(MARKER))?;
            let _ = fs::remove_dir(&directory);
            record.remove();
        }
    }
    Ok(total)
}

#[cfg(unix)]
fn create_staging_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_staging_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn marker_matches(directory: &Path, nonce: &str) -> bool {
    let path = directory.join(MARKER);
    if Uuid::parse_str(nonce).is_err()
        || !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file())
    {
        return false;
    }
    let mut marker = String::new();
    File::open(path)
        .and_then(|file| file.take(37).read_to_string(&mut marker))
        .is_ok()
        && marker == nonce
}

#[cfg(unix)]
fn lock_record(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // Android's std file locks are unsupported even though flock is available.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn lock_record(file: &File) -> io::Result<()> {
    file.try_lock().map_err(io::Error::from)
}
