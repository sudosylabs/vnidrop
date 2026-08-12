use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
    sync::Arc,
};

#[cfg(any(target_os = "android", target_os = "windows", target_os = "linux"))]
use super::map_store_error;
use super::{
    SecretHandle, SecretMaterial, SecureSecretStore, SecureSecretStoreError, HANDLE_NAMESPACE,
    HANDLE_VERSION,
};
use crate::error::VnidropError;

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
static TEST_STORES: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Arc<dyn SecureSecretStore>>>,
> = std::sync::OnceLock::new();

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
pub(crate) fn install_platform_secret_store_for_test(
    app_data_dir: &Path,
    store: Arc<dyn SecureSecretStore>,
) {
    TEST_STORES
        .get_or_init(Default::default)
        .lock()
        .expect("test stores")
        .entry(app_data_dir.to_path_buf())
        .or_insert_with(|| scope_store(app_data_dir, store));
}

#[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
fn platform_secret_store_for_test(app_data_dir: &Path) -> Option<Arc<dyn SecureSecretStore>> {
    TEST_STORES
        .get_or_init(Default::default)
        .lock()
        .expect("test stores")
        .get(app_data_dir)
        .cloned()
}

struct ScopedSecretStore {
    inner: Arc<dyn SecureSecretStore>,
    physical_prefix: String,
}

pub(crate) struct ProfileLock {
    _file: File,
}

pub(crate) fn lock_profile(app_data_dir: &Path) -> Result<ProfileLock, VnidropError> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(app_data_dir.join("protected-secrets.lock"))
        .map_err(VnidropError::filesystem)?;
    lock_exclusive_nonblocking(&file)?;
    Ok(ProfileLock { _file: file })
}

/// Acquire an exclusive advisory lock without blocking.
///
/// Prefer `libc::flock` on Unix: Rust's `File::try_lock` still returns
/// `ErrorKind::Unsupported` on Android even though the kernel supports flock.
fn lock_exclusive_nonblocking(file: &File) -> Result<(), VnidropError> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        Err(match err.kind() {
            io::ErrorKind::WouldBlock => VnidropError::SecureStorageUnavailable {
                reason: "another protected core is already using this profile".to_string(),
            },
            _ => VnidropError::filesystem(err),
        })
    }
    #[cfg(windows)]
    {
        match file.try_lock() {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                Err(VnidropError::SecureStorageUnavailable {
                    reason: "another protected core is already using this profile".to_string(),
                })
            }
            Err(err) => Err(VnidropError::filesystem(err)),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = file;
        Err(VnidropError::SecureStorageUnavailable {
            reason: "profile locking is unsupported on this platform".to_string(),
        })
    }
}

/// Opens the profile marker without locking so in-process restart tests can
/// reopen the same directory after dropping the previous core.
#[cfg(test)]
pub(crate) fn unlocked_profile_for_test(app_data_dir: &Path) -> Result<ProfileLock, VnidropError> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(app_data_dir.join("protected-secrets.lock"))
        .map_err(VnidropError::filesystem)?;
    Ok(ProfileLock { _file: file })
}

impl ScopedSecretStore {
    fn new(app_data_dir: &Path, inner: Arc<dyn SecureSecretStore>) -> Self {
        let profile = blake3::hash(app_data_dir.to_string_lossy().as_bytes()).to_hex();
        Self {
            inner,
            physical_prefix: format!("{HANDLE_NAMESPACE}/{HANDLE_VERSION}/scope-{profile}/"),
        }
    }

    fn physical_handle(
        &self,
        handle: &SecretHandle,
    ) -> Result<SecretHandle, SecureSecretStoreError> {
        let logical_prefix = format!("{HANDLE_NAMESPACE}/{HANDLE_VERSION}/");
        let suffix = handle
            .as_str()
            .strip_prefix(&logical_prefix)
            .ok_or(SecureSecretStoreError::Corrupted)?;
        Ok(SecretHandle(format!("{}{suffix}", self.physical_prefix)))
    }
}

impl SecureSecretStore for ScopedSecretStore {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError> {
        self.inner.put(&self.physical_handle(handle)?, material)
    }

    fn get(&self, handle: &SecretHandle) -> Result<SecretMaterial, SecureSecretStoreError> {
        self.inner.get(&self.physical_handle(handle)?)
    }

    fn delete(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError> {
        self.inner.delete(&self.physical_handle(handle)?)
    }

    fn list_handles(&self) -> Result<Vec<SecretHandle>, SecureSecretStoreError> {
        let handles = self
            .inner
            .list_handles()?
            .into_iter()
            .filter_map(|handle| {
                handle
                    .as_str()
                    .strip_prefix(&self.physical_prefix)
                    .map(|suffix| {
                        SecretHandle(format!("{HANDLE_NAMESPACE}/{HANDLE_VERSION}/{suffix}"))
                    })
            })
            .collect();
        Ok(handles)
    }
}

pub(crate) fn scope_store(
    app_data_dir: &Path,
    store: Arc<dyn SecureSecretStore>,
) -> Arc<dyn SecureSecretStore> {
    Arc::new(ScopedSecretStore::new(app_data_dir, store))
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) fn platform_secret_store(
    app_data_dir: &Path,
) -> Result<Arc<dyn SecureSecretStore>, VnidropError> {
    #[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
    if let Some(store) = platform_secret_store_for_test(app_data_dir) {
        return Ok(store);
    }
    Ok(scope_store(
        app_data_dir,
        Arc::new(super::apple::AppleKeychainSecretStore::new()),
    ))
}

#[cfg(target_os = "android")]
pub(crate) fn platform_secret_store(
    app_data_dir: &Path,
) -> Result<Arc<dyn SecureSecretStore>, VnidropError> {
    #[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
    if let Some(store) = platform_secret_store_for_test(app_data_dir) {
        return Ok(store);
    }
    super::android::native::create_store_from_android_runtime()
        .map(|store| scope_store(app_data_dir, store))
        .map_err(map_store_error)
}

#[cfg(target_os = "windows")]
pub(crate) fn platform_secret_store(
    app_data_dir: &Path,
) -> Result<Arc<dyn SecureSecretStore>, VnidropError> {
    #[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
    if let Some(store) = platform_secret_store_for_test(app_data_dir) {
        return Ok(store);
    }
    super::windows::WindowsDpapiSecretStore::new(app_data_dir.join("protected-secrets-v1"))
        .map(|store| scope_store(app_data_dir, Arc::new(store)))
        .map_err(map_store_error)
}

#[cfg(target_os = "linux")]
pub(crate) fn platform_secret_store(
    app_data_dir: &Path,
) -> Result<Arc<dyn SecureSecretStore>, VnidropError> {
    #[cfg(any(test, all(feature = "integration-test-store", debug_assertions)))]
    if let Some(store) = platform_secret_store_for_test(app_data_dir) {
        return Ok(store);
    }
    super::linux::LinuxSecretServiceStore::connect()
        .map(|store| scope_store(app_data_dir, Arc::new(store)))
        .map_err(map_store_error)
}
