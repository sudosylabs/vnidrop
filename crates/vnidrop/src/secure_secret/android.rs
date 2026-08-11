use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
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
    mutation_lock: Mutex<()>,
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
            mutation_lock: Mutex::new(()),
        })
    }

    fn record_path(&self, handle: &SecretHandle) -> PathBuf {
        // Hash the handle for the on-disk name. Scoped handles are longer than
        // Linux/Android NAME_MAX when hex-encoded, and the handle is already
        // authenticated inside the record body.
        let digest = blake3::hash(handle.as_str().as_bytes());
        let encoded = HEXLOWER.encode(digest.as_bytes());
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

    #[cfg(test)]
    pub(crate) fn record_path_for_test(&self, handle: &SecretHandle) -> PathBuf {
        self.record_path(handle)
    }

    #[cfg(test)]
    pub(crate) fn stage_for_test(
        &self,
        handle: &SecretHandle,
    ) -> Result<(), SecureSecretStoreError> {
        self.write_record(handle, RECORD_STAGED, None)
    }
}

#[cfg(test)]
pub(crate) fn secret_handle_for_test(value: &str) -> SecretHandle {
    SecretHandle(value.to_string())
}

impl SecureSecretStore for AndroidSecureSecretStore {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError> {
        let _mutation = self
            .mutation_lock
            .lock()
            .map_err(|_| SecureSecretStoreError::Unavailable)?;
        let record_exists = match fs::metadata(self.record_path(handle)) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(map_io_error(error)),
        };
        if !record_exists {
            self.write_record(handle, RECORD_STAGED, None)?;
        }
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
        let _mutation = self
            .mutation_lock
            .lock()
            .map_err(|_| SecureSecretStoreError::Unavailable)?;
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
            let mut bytes = Vec::new();
            File::open(&path)
                .map_err(map_io_error)?
                .read_to_end(&mut bytes)
                .map_err(map_io_error)?;
            handles.push(decode_handle_from_record(&bytes)?);
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
    let (handle, state, nonce, ciphertext) = parse_record(bytes)?;
    if handle.as_str() != expected_handle.as_str() || state != RECORD_SEALED {
        return Err(SecureSecretStoreError::Corrupted);
    }
    if nonce.is_empty() || ciphertext.is_empty() {
        return Err(SecureSecretStoreError::Corrupted);
    }
    Ok(AndroidSealedValue { nonce, ciphertext })
}

fn decode_handle_from_record(bytes: &[u8]) -> Result<SecretHandle, SecureSecretStoreError> {
    let (handle, _state, _nonce, _ciphertext) = parse_record(bytes)?;
    Ok(handle)
}

fn parse_record(
    bytes: &[u8],
) -> Result<(SecretHandle, u8, Vec<u8>, Vec<u8>), SecureSecretStoreError> {
    const HEADER_LEN: usize = 8 + 1 + 2 + 2 + 4;
    if bytes.len() < HEADER_LEN || &bytes[..8] != RECORD_MAGIC {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let state = bytes[8];
    if state != RECORD_STAGED && state != RECORD_SEALED {
        return Err(SecureSecretStoreError::Corrupted);
    }
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
    if bytes.len() != expected_len {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let handle_end = HEADER_LEN + handle_len;
    let handle = std::str::from_utf8(&bytes[HEADER_LEN..handle_end])
        .map_err(|_| SecureSecretStoreError::Corrupted)?;
    let nonce_end = handle_end + nonce_len;
    Ok((
        SecretHandle(handle.to_string()),
        state,
        bytes[handle_end..nonce_end].to_vec(),
        bytes[nonce_end..].to_vec(),
    ))
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
