use crate::error::Result;
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

fn temporary_files(root: &Path) -> Result<Vec<(PathBuf, u64)>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        visited += 1;
        if visited > 100_000 {
            return Err("error_invalid_input");
        }
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("error_filesystem"),
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path).map_err(|_| "error_filesystem")? {
                pending.push(entry.map_err(|_| "error_filesystem")?.path());
            }
        } else if metadata.is_file() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let old = metadata
                .modified()
                .ok()
                .and_then(|time| SystemTime::now().duration_since(time).ok())
                .is_some_and(|age| age >= Duration::from_secs(86400));
            if name.starts_with('.') && name.contains(".vnidrop-") && name.ends_with(".part") && old
            {
                files.push((path, metadata.len()));
            }
        }
    }
    Ok(files)
}
pub fn temporary_usage(root: &Path) -> Result<u64> {
    Ok(temporary_files(root)?.iter().map(|(_, size)| size).sum())
}
pub fn cleanup(root: &Path) -> Result<u64> {
    let mut freed = 0;
    for (path, bytes) in temporary_files(root)? {
        std::fs::remove_file(path).map_err(|_| "error_filesystem")?;
        freed += bytes;
    }
    Ok(freed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_keeps_delivered_files_recent_partials_and_symlink_targets() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let stale = root.path().join(".old.vnidrop-123.part");
        std::fs::write(&stale, b"old").unwrap();
        let file = std::fs::File::options().write(true).open(&stale).unwrap();
        file.set_times(
            std::fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(90000)),
        )
        .unwrap();
        std::fs::write(root.path().join("received.txt"), b"keep").unwrap();
        std::fs::write(root.path().join(".new.vnidrop-123.part"), b"keep").unwrap();
        std::fs::write(external.path().join("keep"), b"keep").unwrap();
        std::os::unix::fs::symlink(external.path(), root.path().join("linked")).unwrap();
        assert_eq!(temporary_usage(root.path()).unwrap(), 3);
        assert_eq!(cleanup(root.path()).unwrap(), 3);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 3);
        assert!(external.path().join("keep").exists());
    }
}
