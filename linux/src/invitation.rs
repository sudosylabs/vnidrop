use std::{fs::File, io::Read, path::Path};

use crate::error::Result;

pub const MAX_BYTES: usize = 64 * 1024;

pub fn decode(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_BYTES {
        return Err("error_invalid_ticket");
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "error_invalid_ticket")?;
    if text.trim().is_empty() {
        return Err("error_invitation_empty");
    }
    Ok(text.to_owned())
}

pub fn read(path: &Path) -> Result<String> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("vnd"))
    {
        return Err("error_invalid_ticket");
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
        .map_err(|_| "error_filesystem")?;
    decode(&bytes)
}

pub fn file_name(name: &str) -> String {
    let stem: String = name
        .chars()
        .filter(|ch| !ch.is_control() && !"/\\:*?\"<>|".contains(*ch))
        .take(100)
        .collect();
    let stem = stem.trim().trim_matches('.');
    format!("{}.vnd", if stem.is_empty() { "VniDrop" } else { stem })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_binary_empty_and_oversized_invitations_before_core_inspection() {
        assert_eq!(decode(b" \n\t"), Err("error_invitation_empty"));
        assert_eq!(decode(&[0xff]), Err("error_invalid_ticket"));
        assert_eq!(
            decode(&vec![b'a'; MAX_BYTES + 1]),
            Err("error_invalid_ticket")
        );
        assert_eq!(decode("invitation-é".as_bytes()), Ok("invitation-é".into()));
    }

    #[test]
    fn bounds_file_reads_and_accepts_case_insensitive_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invitation.VND");
        std::fs::write(&path, b"ticket").unwrap();
        assert_eq!(read(&path), Ok("ticket".into()));
        std::fs::write(&path, vec![b'a'; MAX_BYTES + 1]).unwrap();
        assert_eq!(read(&path), Err("error_invalid_ticket"));
        assert_eq!(file_name("../../holiday/family"), "holidayfamily.vnd");
    }
}
