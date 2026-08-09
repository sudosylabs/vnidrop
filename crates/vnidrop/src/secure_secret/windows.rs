use std::{
    collections::HashSet,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
    sync::Arc,
};

use data_encoding::HEXLOWER;
use windows_sys::Win32::{
    Foundation::{
        GetLastError, LocalFree, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS,
        ERROR_CALL_NOT_IMPLEMENTED, ERROR_FILE_EXISTS, ERROR_NOT_SUPPORTED,
        ERROR_PASSWORD_RESTRICTION,
    },
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
    Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH},
};

use super::{SecretHandle, SecretMaterial, SecureSecretStore, SecureSecretStoreError};

#[cfg(test)]
use super::SecretKind;

const ENVELOPE_MAGIC: &[u8; 8] = b"VNIDPAPI";
const ENVELOPE_VERSION: u8 = 1;
const FILE_EXTENSION: &str = "dpapi";
const MAX_ENVELOPE_BYTES: usize = 64 * 1024;
const DEFAULT_CONTEXT: &[u8] = b"com.vnidrop.secure-secret.dpapi.v1.current-user";
const PROTECTED_PAYLOAD_MAGIC: &[u8] = b"VNIDROP-SECRET-V1";

/// Current-user DPAPI storage backed by atomically published protected blobs.
pub(crate) struct WindowsDpapiSecretStore {
    directory: PathBuf,
    protector: Arc<DpapiProtector>,
}

impl WindowsDpapiSecretStore {
    pub(crate) fn new(directory: impl AsRef<Path>) -> Result<Self, SecureSecretStoreError> {
        Self::with_protector(directory, Arc::new(DpapiProtector::new()))
    }

    fn with_protector(
        directory: impl AsRef<Path>,
        protector: Arc<DpapiProtector>,
    ) -> Result<Self, SecureSecretStoreError> {
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory).map_err(map_io_error)?;
        cleanup_interrupted_writes(&directory)?;
        Ok(Self {
            directory,
            protector,
        })
    }

    fn path_for(&self, handle: &SecretHandle) -> PathBuf {
        let digest = blake3::hash(handle.as_str().as_bytes());
        self.directory.join(format!(
            "{}.{}",
            HEXLOWER.encode(digest.as_bytes()),
            FILE_EXTENSION
        ))
    }

    #[cfg(test)]
    pub(crate) fn with_context_for_test(
        directory: impl AsRef<Path>,
        context: &[u8],
    ) -> Result<Self, SecureSecretStoreError> {
        Self::with_protector(
            directory,
            Arc::new(DpapiProtector::with_context_for_test(context)),
        )
    }

    #[cfg(test)]
    pub(crate) fn path_for_test(&self, handle: &SecretHandle) -> PathBuf {
        self.path_for(handle)
    }

    #[cfg(test)]
    pub(crate) fn relationship_handle_for_test() -> SecretHandle {
        SecretHandle::generate(SecretKind::RelationshipGrant)
    }
}

impl SecureSecretStore for WindowsDpapiSecretStore {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError> {
        let destination = self.path_for(handle);
        let replace_existing = match self.get(handle) {
            Ok(existing) if existing == material => return Ok(()),
            Ok(_) => true,
            Err(SecureSecretStoreError::Missing) => false,
            Err(error) => return Err(error),
        };

        let ciphertext = self.protector.protect(handle, &material.0)?;
        let envelope = encode_envelope(handle, &ciphertext)?;
        let temporary = self.directory.join(format!(
            "{}.tmp-{}",
            destination
                .file_stem()
                .and_then(OsStr::to_str)
                .ok_or(SecureSecretStoreError::Unavailable)?,
            uuid::Uuid::new_v4()
        ));
        let mut temporary_guard = TemporaryFile::new(temporary);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary_guard.path())
            .map_err(map_io_error)?;
        file.write_all(&envelope).map_err(map_io_error)?;
        file.sync_all().map_err(map_io_error)?;
        drop(file);

        match move_write_through(temporary_guard.path(), &destination, replace_existing) {
            Ok(()) => {
                temporary_guard.disarm();
                Ok(())
            }
            Err(error)
                if matches!(
                    error.raw_os_error().map(|code| code as u32),
                    Some(ERROR_ALREADY_EXISTS) | Some(ERROR_FILE_EXISTS)
                ) =>
            {
                match self.get(handle) {
                    Ok(existing) if existing == material => Ok(()),
                    Ok(_) => {
                        move_write_through(temporary_guard.path(), &destination, true)
                            .map_err(map_io_error)?;
                        temporary_guard.disarm();
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(map_io_error(error)),
        }
    }

    fn get(&self, handle: &SecretHandle) -> Result<SecretMaterial, SecureSecretStoreError> {
        let envelope = fs::read(self.path_for(handle)).map_err(map_io_error)?;
        let ciphertext = decode_envelope(&envelope, handle)?;
        let plaintext = self.protector.unprotect(handle, ciphertext)?;
        SecretMaterial::new(plaintext).map_err(|_| SecureSecretStoreError::Corrupted)
    }

    fn delete(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError> {
        fs::remove_file(self.path_for(handle)).map_err(map_io_error)
    }

    fn list_handles(&self) -> Result<Vec<SecretHandle>, SecureSecretStoreError> {
        let mut handles = Vec::new();
        let mut unique = HashSet::new();
        for entry in fs::read_dir(&self.directory).map_err(map_io_error)? {
            let entry = entry.map_err(map_io_error)?;
            let path = entry.path();
            if path.extension() != Some(OsStr::new(FILE_EXTENSION)) {
                continue;
            }
            let envelope = fs::read(&path).map_err(map_io_error)?;
            let (handle, _) = decode_envelope_parts(&envelope)?;
            if self.path_for(&handle) != path || !unique.insert(handle.clone()) {
                return Err(SecureSecretStoreError::Corrupted);
            }
            handles.push(handle);
        }
        handles.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        Ok(handles)
    }
}

fn encode_envelope(
    handle: &SecretHandle,
    ciphertext: &[u8],
) -> Result<Vec<u8>, SecureSecretStoreError> {
    let handle_bytes = handle.as_str().as_bytes();
    let handle_len =
        u16::try_from(handle_bytes.len()).map_err(|_| SecureSecretStoreError::Unavailable)?;
    let ciphertext_len =
        u32::try_from(ciphertext.len()).map_err(|_| SecureSecretStoreError::Unavailable)?;
    let capacity = ENVELOPE_MAGIC.len() + 1 + 2 + 4 + handle_bytes.len() + ciphertext.len();
    if capacity > MAX_ENVELOPE_BYTES {
        return Err(SecureSecretStoreError::Unavailable);
    }
    let mut envelope = Vec::with_capacity(capacity);
    envelope.extend_from_slice(ENVELOPE_MAGIC);
    envelope.push(ENVELOPE_VERSION);
    envelope.extend_from_slice(&handle_len.to_le_bytes());
    envelope.extend_from_slice(&ciphertext_len.to_le_bytes());
    envelope.extend_from_slice(handle_bytes);
    envelope.extend_from_slice(ciphertext);
    Ok(envelope)
}

fn decode_envelope<'a>(
    envelope: &'a [u8],
    expected_handle: &SecretHandle,
) -> Result<&'a [u8], SecureSecretStoreError> {
    let (handle, ciphertext) = decode_envelope_parts(envelope)?;
    if handle != *expected_handle {
        return Err(SecureSecretStoreError::Corrupted);
    }
    Ok(ciphertext)
}

fn decode_envelope_parts(envelope: &[u8]) -> Result<(SecretHandle, &[u8]), SecureSecretStoreError> {
    const HEADER_BYTES: usize = 8 + 1 + 2 + 4;
    if envelope.len() < HEADER_BYTES
        || envelope.len() > MAX_ENVELOPE_BYTES
        || &envelope[..8] != ENVELOPE_MAGIC
        || envelope[8] != ENVELOPE_VERSION
    {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let handle_len = usize::from(u16::from_le_bytes([envelope[9], envelope[10]]));
    let ciphertext_len =
        u32::from_le_bytes([envelope[11], envelope[12], envelope[13], envelope[14]]) as usize;
    let handle_end = HEADER_BYTES
        .checked_add(handle_len)
        .ok_or(SecureSecretStoreError::Corrupted)?;
    let envelope_end = handle_end
        .checked_add(ciphertext_len)
        .ok_or(SecureSecretStoreError::Corrupted)?;
    if handle_len == 0 || ciphertext_len == 0 || envelope_end != envelope.len() {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let handle = std::str::from_utf8(&envelope[HEADER_BYTES..handle_end])
        .map_err(|_| SecureSecretStoreError::Corrupted)?;
    Ok((
        SecretHandle(handle.to_owned()),
        &envelope[handle_end..envelope_end],
    ))
}

fn cleanup_interrupted_writes(directory: &Path) -> Result<(), SecureSecretStoreError> {
    for entry in fs::read_dir(directory).map_err(map_io_error)? {
        let entry = entry.map_err(map_io_error)?;
        let name = entry.file_name();
        if name.to_string_lossy().contains(".tmp-") {
            fs::remove_file(entry.path()).map_err(map_io_error)?;
        }
    }
    Ok(())
}

struct TemporaryFile {
    path: PathBuf,
    armed: bool,
}

impl TemporaryFile {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn move_write_through(source: &Path, destination: &Path, replace_existing: bool) -> io::Result<()> {
    let source = wide_path(source);
    let destination = wide_path(destination);
    // The files share a directory, so MoveFileEx publishes the fully flushed blob as one rename.
    let flags = if replace_existing {
        MOVEFILE_WRITE_THROUGH | MOVEFILE_REPLACE_EXISTING
    } else {
        MOVEFILE_WRITE_THROUGH
    };
    let moved = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

struct DpapiProtector {
    context: Vec<u8>,
}

impl DpapiProtector {
    fn new() -> Self {
        Self {
            context: DEFAULT_CONTEXT.to_vec(),
        }
    }

    #[cfg(test)]
    fn with_context_for_test(context: &[u8]) -> Self {
        Self {
            context: context.to_vec(),
        }
    }

    fn entropy(&self, handle: &SecretHandle) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.context);
        hasher.update(&[0]);
        hasher.update(handle.as_str().as_bytes());
        *hasher.finalize().as_bytes()
    }

    fn protect(
        &self,
        handle: &SecretHandle,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecureSecretStoreError> {
        let mut payload = encode_protected_payload(plaintext)?;
        let input = blob(&payload)?;
        let entropy_bytes = self.entropy(handle);
        let entropy = blob(&entropy_bytes)?;
        let mut output = CRYPT_INTEGER_BLOB::default();
        // Omitting CRYPTPROTECT_LOCAL_MACHINE binds the blob to the current Windows user.
        let success = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        let result = if success == 0 {
            Err(map_dpapi_error(false))
        } else {
            copy_and_free(output, false)
        };
        payload.fill(0);
        result
    }

    fn unprotect(
        &self,
        handle: &SecretHandle,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, SecureSecretStoreError> {
        let input = blob(ciphertext)?;
        let entropy_bytes = self.entropy(handle);
        let entropy = blob(&entropy_bytes)?;
        let mut output = CRYPT_INTEGER_BLOB::default();
        let success = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if success == 0 {
            return Err(map_dpapi_error(true));
        }
        let mut payload = copy_and_free(output, true)?;
        let plaintext = decode_protected_payload(&payload).map(<[u8]>::to_vec);
        payload.fill(0);
        plaintext
    }
}

fn encode_protected_payload(plaintext: &[u8]) -> Result<Vec<u8>, SecureSecretStoreError> {
    let length = u32::try_from(plaintext.len()).map_err(|_| SecureSecretStoreError::Unavailable)?;
    let mut payload = Vec::with_capacity(PROTECTED_PAYLOAD_MAGIC.len() + 4 + plaintext.len() + 32);
    payload.extend_from_slice(PROTECTED_PAYLOAD_MAGIC);
    payload.extend_from_slice(&length.to_le_bytes());
    payload.extend_from_slice(plaintext);
    let digest = blake3::hash(&payload);
    payload.extend_from_slice(digest.as_bytes());
    Ok(payload)
}

fn decode_protected_payload(payload: &[u8]) -> Result<&[u8], SecureSecretStoreError> {
    let header_end = PROTECTED_PAYLOAD_MAGIC.len() + 4;
    if payload.len() < header_end + 32 || !payload.starts_with(PROTECTED_PAYLOAD_MAGIC) {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let length = u32::from_le_bytes(
        payload[PROTECTED_PAYLOAD_MAGIC.len()..header_end]
            .try_into()
            .map_err(|_| SecureSecretStoreError::Corrupted)?,
    ) as usize;
    let material_end = header_end
        .checked_add(length)
        .ok_or(SecureSecretStoreError::Corrupted)?;
    if material_end
        .checked_add(32)
        .ok_or(SecureSecretStoreError::Corrupted)?
        != payload.len()
    {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let expected = blake3::hash(&payload[..material_end]);
    if expected.as_bytes() != &payload[material_end..] {
        return Err(SecureSecretStoreError::Corrupted);
    }
    Ok(&payload[header_end..material_end])
}

fn blob(bytes: &[u8]) -> Result<CRYPT_INTEGER_BLOB, SecureSecretStoreError> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| SecureSecretStoreError::Unavailable)?,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

fn copy_and_free(
    output: CRYPT_INTEGER_BLOB,
    clear_before_free: bool,
) -> Result<Vec<u8>, SecureSecretStoreError> {
    if output.pbData.is_null() || output.cbData == 0 {
        return Err(SecureSecretStoreError::Corrupted);
    }
    let result = unsafe {
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        if clear_before_free {
            ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        }
        LocalFree(output.pbData.cast());
        bytes
    };
    Ok(result)
}

fn map_dpapi_error(unprotecting: bool) -> SecureSecretStoreError {
    let code = unsafe { GetLastError() };
    match code {
        ERROR_ACCESS_DENIED | ERROR_PASSWORD_RESTRICTION => SecureSecretStoreError::Locked,
        ERROR_NOT_SUPPORTED | ERROR_CALL_NOT_IMPLEMENTED => SecureSecretStoreError::Unavailable,
        _ if unprotecting => SecureSecretStoreError::Corrupted,
        _ => SecureSecretStoreError::Unavailable,
    }
}

fn map_io_error(error: io::Error) -> SecureSecretStoreError {
    match error.kind() {
        io::ErrorKind::NotFound => SecureSecretStoreError::Missing,
        io::ErrorKind::PermissionDenied => SecureSecretStoreError::Locked,
        io::ErrorKind::InvalidData => SecureSecretStoreError::Corrupted,
        _ => SecureSecretStoreError::Unavailable,
    }
}
