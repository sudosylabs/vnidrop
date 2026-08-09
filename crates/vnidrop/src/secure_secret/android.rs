use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use data_encoding::HEXLOWER;

use super::{SecretHandle, SecretMaterial, SecureSecretStore, SecureSecretStoreError};

const RECORD_MAGIC: &[u8; 8] = b"VNDASK01";
const RECORD_STAGED: u8 = 0;
const RECORD_SEALED: u8 = 1;
const RECORD_EXTENSION: &str = "vns";
const KEY_ALIAS_PREFIX: &str = "vnidrop.secret.v1.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidSealedValue {
    pub(crate) nonce: Vec<u8>,
    pub(crate) ciphertext: Vec<u8>,
}

/// Performs AES-GCM operations with a non-exportable key held by Android Keystore.
///
/// Implementations create one key per alias, let Keystore generate the encryption
/// nonce, and never return key material to Rust.
pub(crate) trait AndroidKeystore: Send + Sync {
    fn seal(
        &self,
        alias: &str,
        plaintext: &[u8],
    ) -> Result<AndroidSealedValue, SecureSecretStoreError>;
    fn open(
        &self,
        alias: &str,
        sealed: &AndroidSealedValue,
    ) -> Result<Vec<u8>, SecureSecretStoreError>;
    fn delete(&self, alias: &str) -> Result<(), SecureSecretStoreError>;
}

/// Android secret-store adapter whose ordinary storage contains authenticated
/// ciphertext only.
///
/// `no_backup_dir` must be the directory returned by Android
/// `Context.getNoBackupFilesDir()`. The Android host owns acquiring that Context;
/// secret values never cross that initialization boundary.
pub(crate) struct AndroidSecureSecretStore {
    records_dir: PathBuf,
    keystore: Arc<dyn AndroidKeystore>,
}

impl AndroidSecureSecretStore {
    pub(crate) fn new(
        no_backup_dir: &Path,
        keystore: Arc<dyn AndroidKeystore>,
    ) -> Result<Self, SecureSecretStoreError> {
        if !no_backup_dir.is_absolute() {
            return Err(SecureSecretStoreError::Unavailable);
        }
        let records_dir = no_backup_dir.join("vnidrop-protected-secrets-v1");
        fs::create_dir_all(&records_dir).map_err(map_io_error)?;
        set_private_directory_permissions(&records_dir)?;
        Ok(Self {
            records_dir,
            keystore,
        })
    }

    fn record_path(&self, handle: &SecretHandle) -> PathBuf {
        let encoded = HEXLOWER.encode(handle.as_str().as_bytes());
        self.records_dir
            .join(format!("{encoded}.{RECORD_EXTENSION}"))
    }

    fn alias(handle: &SecretHandle) -> String {
        let digest = blake3::hash(handle.as_str().as_bytes());
        format!("{KEY_ALIAS_PREFIX}{}", HEXLOWER.encode(digest.as_bytes()))
    }

    fn write_record(
        &self,
        handle: &SecretHandle,
        state: u8,
        sealed: Option<&AndroidSealedValue>,
    ) -> Result<(), SecureSecretStoreError> {
        let bytes = encode_record(handle, state, sealed)?;
        let path = self.record_path(handle);
        let temporary = path.with_extension(format!("{RECORD_EXTENSION}.tmp"));
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(map_io_error)?;
        set_private_file_permissions(&temporary)?;
        file.write_all(&bytes).map_err(map_io_error)?;
        file.sync_all().map_err(map_io_error)?;
        fs::rename(&temporary, &path).map_err(map_io_error)?;
        sync_directory(&self.records_dir)?;
        Ok(())
    }

    fn read_record(
        &self,
        handle: &SecretHandle,
    ) -> Result<AndroidSealedValue, SecureSecretStoreError> {
        let mut bytes = Vec::new();
        File::open(self.record_path(handle))
            .map_err(map_io_error)?
            .read_to_end(&mut bytes)
            .map_err(map_io_error)?;
        decode_record(&bytes, handle)
    }
}

impl SecureSecretStore for AndroidSecureSecretStore {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError> {
        self.write_record(handle, RECORD_STAGED, None)?;
        let alias = Self::alias(handle);
        let sealed = self.keystore.seal(&alias, &material.0)?;
        if sealed.nonce.is_empty() || sealed.ciphertext.is_empty() {
            return Err(SecureSecretStoreError::Corrupted);
        }
        self.write_record(handle, RECORD_SEALED, Some(&sealed))
    }

    fn get(&self, handle: &SecretHandle) -> Result<SecretMaterial, SecureSecretStoreError> {
        let sealed = self.read_record(handle)?;
        let plaintext = self.keystore.open(&Self::alias(handle), &sealed)?;
        SecretMaterial::new(plaintext).map_err(|_| SecureSecretStoreError::Corrupted)
    }

    fn delete(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError> {
        match self.keystore.delete(&Self::alias(handle)) {
            Ok(()) | Err(SecureSecretStoreError::Missing) => {}
            Err(error) => return Err(error),
        }
        let path = self.record_path(handle);
        match fs::remove_file(path) {
            Ok(()) => sync_directory(&self.records_dir)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(map_io_error(error)),
        }
        Ok(())
    }

    fn list_handles(&self) -> Result<Vec<SecretHandle>, SecureSecretStoreError> {
        let mut handles = Vec::new();
        for entry in fs::read_dir(&self.records_dir).map_err(map_io_error)? {
            let entry = entry.map_err(map_io_error)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some(RECORD_EXTENSION) {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or(SecureSecretStoreError::Corrupted)?;
            let decoded = HEXLOWER
                .decode(stem.as_bytes())
                .map_err(|_| SecureSecretStoreError::Corrupted)?;
            let handle =
                String::from_utf8(decoded).map_err(|_| SecureSecretStoreError::Corrupted)?;
            handles.push(SecretHandle(handle));
        }
        handles.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        Ok(handles)
    }
}

fn encode_record(
    handle: &SecretHandle,
    state: u8,
    sealed: Option<&AndroidSealedValue>,
) -> Result<Vec<u8>, SecureSecretStoreError> {
    let handle_bytes = handle.as_str().as_bytes();
    let handle_len =
        u16::try_from(handle_bytes.len()).map_err(|_| SecureSecretStoreError::Corrupted)?;
    let (nonce, ciphertext) = match (state, sealed) {
        (RECORD_STAGED, None) => (&[][..], &[][..]),
        (RECORD_SEALED, Some(value)) => (value.nonce.as_slice(), value.ciphertext.as_slice()),
        _ => return Err(SecureSecretStoreError::Corrupted),
    };
    let nonce_len = u16::try_from(nonce.len()).map_err(|_| SecureSecretStoreError::Corrupted)?;
    let ciphertext_len =
        u32::try_from(ciphertext.len()).map_err(|_| SecureSecretStoreError::Corrupted)?;
    let mut record = Vec::with_capacity(
        RECORD_MAGIC.len() + 1 + 2 + 2 + 4 + handle_bytes.len() + nonce.len() + ciphertext.len(),
    );
    record.extend_from_slice(RECORD_MAGIC);
    record.push(state);
    record.extend_from_slice(&handle_len.to_be_bytes());
    record.extend_from_slice(&nonce_len.to_be_bytes());
    record.extend_from_slice(&ciphertext_len.to_be_bytes());
    record.extend_from_slice(handle_bytes);
    record.extend_from_slice(nonce);
    record.extend_from_slice(ciphertext);
    Ok(record)
}

fn decode_record(
    bytes: &[u8],
    expected_handle: &SecretHandle,
) -> Result<AndroidSealedValue, SecureSecretStoreError> {
    const HEADER_LEN: usize = 8 + 1 + 2 + 2 + 4;
    if bytes.len() < HEADER_LEN || &bytes[..8] != RECORD_MAGIC {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let state = bytes[8];
    let handle_len = usize::from(u16::from_be_bytes([bytes[9], bytes[10]]));
    let nonce_len = usize::from(u16::from_be_bytes([bytes[11], bytes[12]]));
    let ciphertext_len = usize::try_from(u32::from_be_bytes([
        bytes[13], bytes[14], bytes[15], bytes[16],
    ]))
    .map_err(|_| SecureSecretStoreError::Corrupted)?;
    let expected_len = HEADER_LEN
        .checked_add(handle_len)
        .and_then(|value| value.checked_add(nonce_len))
        .and_then(|value| value.checked_add(ciphertext_len))
        .ok_or(SecureSecretStoreError::Corrupted)?;
    if bytes.len() != expected_len || state != RECORD_SEALED {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let handle_end = HEADER_LEN + handle_len;
    let handle = std::str::from_utf8(&bytes[HEADER_LEN..handle_end])
        .map_err(|_| SecureSecretStoreError::Corrupted)?;
    if handle != expected_handle.as_str() {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let nonce_end = handle_end + nonce_len;
    if nonce_len == 0 || ciphertext_len == 0 {
        return Err(SecureSecretStoreError::Corrupted);
    }
    Ok(AndroidSealedValue {
        nonce: bytes[handle_end..nonce_end].to_vec(),
        ciphertext: bytes[nonce_end..].to_vec(),
    })
}

fn map_io_error(error: std::io::Error) -> SecureSecretStoreError {
    match error.kind() {
        std::io::ErrorKind::NotFound => SecureSecretStoreError::Missing,
        std::io::ErrorKind::InvalidData => SecureSecretStoreError::Corrupted,
        _ => SecureSecretStoreError::Unavailable,
    }
}

fn sync_directory(path: &Path) -> Result<(), SecureSecretStoreError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(map_io_error)
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), SecureSecretStoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(map_io_error)
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), SecureSecretStoreError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), SecureSecretStoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(map_io_error)
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), SecureSecretStoreError> {
    Ok(())
}

#[cfg(target_os = "android")]
#[path = "android_native.rs"]
pub(crate) mod native;

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use tempfile::TempDir;

    use super::*;
    use crate::secure_secret::SECRET_BYTES;

    #[derive(Default)]
    struct FakeKeystore {
        keys: Mutex<HashMap<String, u8>>,
        delete_failure: Mutex<Option<SecureSecretStoreError>>,
    }

    impl AndroidKeystore for FakeKeystore {
        fn seal(
            &self,
            alias: &str,
            plaintext: &[u8],
        ) -> Result<AndroidSealedValue, SecureSecretStoreError> {
            let mask = 0xa7;
            self.keys.lock().unwrap().insert(alias.to_string(), mask);
            Ok(AndroidSealedValue {
                nonce: vec![4; 12],
                ciphertext: plaintext.iter().map(|byte| byte ^ mask).collect(),
            })
        }

        fn open(
            &self,
            alias: &str,
            sealed: &AndroidSealedValue,
        ) -> Result<Vec<u8>, SecureSecretStoreError> {
            let mask = *self
                .keys
                .lock()
                .unwrap()
                .get(alias)
                .ok_or(SecureSecretStoreError::Missing)?;
            Ok(sealed.ciphertext.iter().map(|byte| byte ^ mask).collect())
        }

        fn delete(&self, alias: &str) -> Result<(), SecureSecretStoreError> {
            if let Some(error) = self.delete_failure.lock().unwrap().take() {
                return Err(error);
            }
            self.keys.lock().unwrap().remove(alias);
            Ok(())
        }
    }

    fn fixture() -> (TempDir, AndroidSecureSecretStore, Arc<FakeKeystore>) {
        let directory = TempDir::new().unwrap();
        let keystore = Arc::new(FakeKeystore::default());
        let store = AndroidSecureSecretStore::new(directory.path(), keystore.clone()).unwrap();
        (directory, store, keystore)
    }

    fn handle() -> SecretHandle {
        SecretHandle("vnidrop/v1/endpoint-identity/test".to_string())
    }

    #[test]
    fn adapter_round_trips_lists_and_deletes_without_plaintext_persistence() {
        let (directory, store, keystore) = fixture();
        let handle = handle();
        let plaintext = vec![0x5a; SECRET_BYTES];

        store
            .put(&handle, SecretMaterial::new(plaintext.clone()).unwrap())
            .unwrap();

        let persisted = fs::read(store.record_path(&handle)).unwrap();
        assert!(!persisted
            .windows(plaintext.len())
            .any(|window| window == plaintext));

        drop(store);
        let restarted = AndroidSecureSecretStore::new(directory.path(), keystore).unwrap();
        assert_eq!(restarted.list_handles().unwrap(), vec![handle.clone()]);
        assert_eq!(restarted.get(&handle).unwrap().0, plaintext);

        restarted.delete(&handle).unwrap();
        assert!(restarted.list_handles().unwrap().is_empty());
        assert!(matches!(
            restarted.get(&handle),
            Err(SecureSecretStoreError::Missing)
        ));
    }

    #[test]
    fn staged_crash_record_remains_discoverable_and_fails_closed() {
        let (_directory, store, _keystore) = fixture();
        let handle = handle();
        store.write_record(&handle, RECORD_STAGED, None).unwrap();

        assert_eq!(store.list_handles().unwrap(), vec![handle.clone()]);
        assert!(matches!(
            store.get(&handle),
            Err(SecureSecretStoreError::Corrupted)
        ));
        store.delete(&handle).unwrap();
        assert!(store.list_handles().unwrap().is_empty());
    }

    #[test]
    fn tampering_and_missing_keystore_keys_are_distinct_failures() {
        let (_directory, store, keystore) = fixture();
        let handle = handle();
        store
            .put(&handle, SecretMaterial::new(vec![9; SECRET_BYTES]).unwrap())
            .unwrap();

        keystore.keys.lock().unwrap().clear();
        assert!(matches!(
            store.get(&handle),
            Err(SecureSecretStoreError::Missing)
        ));

        fs::write(store.record_path(&handle), b"tampered").unwrap();
        assert!(matches!(
            store.get(&handle),
            Err(SecureSecretStoreError::Corrupted)
        ));
    }

    #[test]
    fn failed_key_deletion_retains_the_record_for_safe_retry() {
        let (_directory, store, keystore) = fixture();
        let handle = handle();
        store
            .put(&handle, SecretMaterial::new(vec![7; SECRET_BYTES]).unwrap())
            .unwrap();
        *keystore.delete_failure.lock().unwrap() = Some(SecureSecretStoreError::Locked);

        assert!(matches!(
            store.delete(&handle),
            Err(SecureSecretStoreError::Locked)
        ));
        assert_eq!(store.list_handles().unwrap(), vec![handle.clone()]);

        store.delete(&handle).unwrap();
        assert!(store.list_handles().unwrap().is_empty());
    }
}
