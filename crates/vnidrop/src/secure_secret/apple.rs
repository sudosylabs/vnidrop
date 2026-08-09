use super::{SecretHandle, SecretMaterial, SecureSecretStore, SecureSecretStoreError};
use security_framework::{
    access_control::{ProtectionMode, SecAccessControl},
    item::{ItemClass, ItemSearchOptions, Limit, SearchResult},
    passwords::{
        delete_generic_password_options, generic_password, set_generic_password_options,
        PasswordOptions,
    },
};
use std::sync::Arc;

const SERVICE: &str = "com.vnidrop.secure-secrets.v1";
const ACCOUNT_ATTRIBUTE: &str = "acct";
const ERR_SEC_PARAM: i32 = -50;
const ERR_SEC_AUTH_FAILED: i32 = -25_293;
const ERR_SEC_NOT_AVAILABLE: i32 = -25_291;
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25_300;
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25_308;
const ERR_SEC_DECODE: i32 = -26_275;
const ERR_SEC_MISSING_ENTITLEMENT: i32 = -34_018;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppleAccessibility {
    AfterFirstUnlockThisDeviceOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AppleKeychainPolicy {
    accessibility: AppleAccessibility,
    synchronizable: bool,
    data_protection_keychain: bool,
}

impl Default for AppleKeychainPolicy {
    fn default() -> Self {
        Self {
            accessibility: AppleAccessibility::AfterFirstUnlockThisDeviceOnly,
            synchronizable: false,
            data_protection_keychain: true,
        }
    }
}

pub(crate) trait AppleKeychainApi: Send + Sync {
    fn put(
        &self,
        service: &str,
        account: &str,
        material: &[u8],
        policy: AppleKeychainPolicy,
    ) -> Result<(), i32>;
    fn get(&self, service: &str, account: &str) -> Result<Vec<u8>, i32>;
    fn delete(&self, service: &str, account: &str) -> Result<(), i32>;
    fn list_accounts(&self, service: &str) -> Result<Vec<String>, i32>;
}

#[derive(Default)]
struct SystemAppleKeychain;

impl SystemAppleKeychain {
    fn options(service: &str, account: &str) -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(service, account);
        options.set_access_synchronized(Some(false));
        options.use_protected_keychain();
        options
    }
}

impl AppleKeychainApi for SystemAppleKeychain {
    fn put(
        &self,
        service: &str,
        account: &str,
        material: &[u8],
        policy: AppleKeychainPolicy,
    ) -> Result<(), i32> {
        debug_assert_eq!(policy, AppleKeychainPolicy::default());
        let access_control = SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleAfterFirstUnlockThisDeviceOnly),
            0,
        )
        .map_err(|error| error.code())?;
        let mut options = Self::options(service, account);
        options.set_access_control(access_control);
        set_generic_password_options(material, options).map_err(|error| error.code())
    }

    fn get(&self, service: &str, account: &str) -> Result<Vec<u8>, i32> {
        generic_password(Self::options(service, account)).map_err(|error| error.code())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), i32> {
        delete_generic_password_options(Self::options(service, account))
            .map_err(|error| error.code())
    }

    fn list_accounts(&self, service: &str) -> Result<Vec<String>, i32> {
        let mut options = ItemSearchOptions::new();
        options
            .class(ItemClass::generic_password())
            .service(service)
            .cloud_sync(Some(false))
            .load_attributes(true)
            .limit(Limit::All);
        #[cfg(target_os = "macos")]
        options.ignore_legacy_keychains();

        let results = match options.search() {
            Ok(results) => results,
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => return Ok(Vec::new()),
            Err(error) => return Err(error.code()),
        };
        results
            .into_iter()
            .map(|result| match result {
                SearchResult::Dict(_) => result
                    .simplify_dict()
                    .and_then(|attributes| attributes.get(ACCOUNT_ATTRIBUTE).cloned())
                    .ok_or(ERR_SEC_DECODE),
                _ => Err(ERR_SEC_DECODE),
            })
            .collect()
    }
}

/// Stores VniDrop's protected material in Apple's device-local data-protection Keychain.
pub(crate) struct AppleKeychainSecretStore {
    api: Arc<dyn AppleKeychainApi>,
}

impl AppleKeychainSecretStore {
    pub(crate) fn new() -> Self {
        Self {
            api: Arc::new(SystemAppleKeychain),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_api(api: impl AppleKeychainApi + 'static) -> Self {
        Self { api: Arc::new(api) }
    }
}

impl SecureSecretStore for AppleKeychainSecretStore {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError> {
        self.api
            .put(
                SERVICE,
                handle.as_str(),
                &material.0,
                AppleKeychainPolicy::default(),
            )
            .map_err(map_status)
    }

    fn get(&self, handle: &SecretHandle) -> Result<SecretMaterial, SecureSecretStoreError> {
        let material = self.api.get(SERVICE, handle.as_str()).map_err(map_status)?;
        SecretMaterial::new(material).map_err(|_| SecureSecretStoreError::Corrupted)
    }

    fn delete(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError> {
        self.api
            .delete(SERVICE, handle.as_str())
            .map_err(map_status)
    }

    fn list_handles(&self) -> Result<Vec<SecretHandle>, SecureSecretStoreError> {
        self.api
            .list_accounts(SERVICE)
            .map(|accounts| accounts.into_iter().map(SecretHandle).collect())
            .map_err(map_status)
    }
}

fn map_status(status: i32) -> SecureSecretStoreError {
    match status {
        ERR_SEC_ITEM_NOT_FOUND => SecureSecretStoreError::Missing,
        ERR_SEC_INTERACTION_NOT_ALLOWED | ERR_SEC_AUTH_FAILED => SecureSecretStoreError::Locked,
        ERR_SEC_DECODE | ERR_SEC_PARAM => SecureSecretStoreError::Corrupted,
        ERR_SEC_NOT_AVAILABLE | ERR_SEC_MISSING_ENTITLEMENT => SecureSecretStoreError::Unavailable,
        _ => SecureSecretStoreError::Unavailable,
    }
}

#[cfg(test)]
pub(crate) fn expected_policy_for_test() -> AppleKeychainPolicy {
    AppleKeychainPolicy::default()
}

#[cfg(test)]
pub(crate) fn service_for_test() -> &'static str {
    SERVICE
}

#[cfg(test)]
pub(crate) fn handle_for_test(value: &str) -> SecretHandle {
    SecretHandle(value.to_string())
}

#[cfg(test)]
pub(crate) fn map_status_for_test(status: i32) -> SecureSecretStoreError {
    map_status(status)
}
