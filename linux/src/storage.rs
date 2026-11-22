use std::path::Path;

use crate::error::{message_key, Result};

pub fn temporary_usage(profile: &Path) -> Result<u64> {
    vnidrop::receive_staging_bytes(profile.to_string_lossy().into_owned(), false)
        .map_err(message_key)
}

pub fn cleanup(profile: &Path) -> Result<u64> {
    vnidrop::receive_staging_bytes(profile.to_string_lossy().into_owned(), true)
        .map_err(message_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_preserves_files_that_only_look_temporary() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join(".archive.vnidrop-123.part");
        std::fs::write(&file, b"keep").unwrap();
        assert_eq!(temporary_usage(root.path()).unwrap(), 0);
        assert_eq!(cleanup(root.path()).unwrap(), 0);
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }
}
