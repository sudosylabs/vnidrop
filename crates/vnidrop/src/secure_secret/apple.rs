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
struct AppleKeychainPolicy {
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

trait AppleKeychainApi: Send + Sync {
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
    fn with_api(api: impl AppleKeychainApi + 'static) -> Self {
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
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct RecordingKeychain {
        state: Arc<Mutex<RecordingState>>,
    }

    #[derive(Default)]
    struct RecordingState {
        entries: HashMap<(String, String), Vec<u8>>,
        last_policy: Option<AppleKeychainPolicy>,
    }

    impl AppleKeychainApi for RecordingKeychain {
        fn put(
            &self,
            service: &str,
            account: &str,
            material: &[u8],
            policy: AppleKeychainPolicy,
        ) -> Result<(), i32> {
            let mut state = self.state.lock().unwrap();
            state.last_policy = Some(policy);
            state.entries.insert(
                (service.to_string(), account.to_string()),
                material.to_vec(),
            );
            Ok(())
        }

        fn get(&self, service: &str, account: &str) -> Result<Vec<u8>, i32> {
            self.state
                .lock()
                .unwrap()
                .entries
                .get(&(service.to_string(), account.to_string()))
                .cloned()
                .ok_or(ERR_SEC_ITEM_NOT_FOUND)
        }

        fn delete(&self, service: &str, account: &str) -> Result<(), i32> {
            self.state
                .lock()
                .unwrap()
                .entries
                .remove(&(service.to_string(), account.to_string()))
                .map(|_| ())
                .ok_or(ERR_SEC_ITEM_NOT_FOUND)
        }

        fn list_accounts(&self, service: &str) -> Result<Vec<String>, i32> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .entries
                .keys()
                .filter(|(entry_service, _)| entry_service == service)
                .map(|(_, account)| account.clone())
                .collect())
        }
    }

    fn handle(value: &str) -> SecretHandle {
        SecretHandle(value.to_string())
    }

    #[test]
    fn adapter_creates_replaces_reads_lists_and_deletes_only_its_service() {
        let api = RecordingKeychain::default();
        api.state.lock().unwrap().entries.insert(
            ("com.example.unrelated".to_string(), "leave-me".to_string()),
            vec![0x77; 32],
        );
        let store = AppleKeychainSecretStore::with_api(api.clone());
        let owned = handle("vnidrop/v1/endpoint-identity/apple-test");

        store
            .put(&owned, SecretMaterial::new(vec![0x31; 32]).unwrap())
            .unwrap();
        drop(store);

        let reopened_store = AppleKeychainSecretStore::with_api(api.clone());
        assert_eq!(
            reopened_store.get(&owned).unwrap(),
            SecretMaterial::new(vec![0x31; 32]).unwrap()
        );
        reopened_store
            .put(&owned, SecretMaterial::new(vec![0x42; 32]).unwrap())
            .unwrap();

        assert_eq!(
            reopened_store.get(&owned).unwrap(),
            SecretMaterial::new(vec![0x42; 32]).unwrap()
        );
        assert_eq!(reopened_store.list_handles().unwrap(), vec![owned.clone()]);
        reopened_store.delete(&owned).unwrap();
        assert!(matches!(
            reopened_store.get(&owned),
            Err(SecureSecretStoreError::Missing)
        ));
        assert!(api
            .state
            .lock()
            .unwrap()
            .entries
            .contains_key(&("com.example.unrelated".to_string(), "leave-me".to_string())));
    }

    #[test]
    fn adapter_always_requests_device_local_non_synchronizing_protection() {
        let api = RecordingKeychain::default();
        let store = AppleKeychainSecretStore::with_api(api.clone());

        store
            .put(
                &handle("vnidrop/v1/relationship-grant/apple-policy"),
                SecretMaterial::new(vec![0x51; 32]).unwrap(),
            )
            .unwrap();

        assert_eq!(
            api.state.lock().unwrap().last_policy,
            Some(AppleKeychainPolicy {
                accessibility: AppleAccessibility::AfterFirstUnlockThisDeviceOnly,
                synchronizable: false,
                data_protection_keychain: true,
            })
        );
    }

    #[test]
    fn apple_statuses_map_to_fail_closed_contract_outcomes() {
        assert!(matches!(
            map_status(ERR_SEC_INTERACTION_NOT_ALLOWED),
            SecureSecretStoreError::Locked
        ));
        assert!(matches!(
            map_status(ERR_SEC_AUTH_FAILED),
            SecureSecretStoreError::Locked
        ));
        assert!(matches!(
            map_status(ERR_SEC_ITEM_NOT_FOUND),
            SecureSecretStoreError::Missing
        ));
        assert!(matches!(
            map_status(ERR_SEC_DECODE),
            SecureSecretStoreError::Corrupted
        ));
        assert!(matches!(
            map_status(ERR_SEC_NOT_AVAILABLE),
            SecureSecretStoreError::Unavailable
        ));
        assert!(matches!(
            map_status(-1),
            SecureSecretStoreError::Unavailable
        ));
    }

    #[test]
    fn malformed_keychain_values_are_corrupted_without_diagnostic_disclosure() {
        let api = RecordingKeychain::default();
        let secret = vec![0x6d; 31];
        api.state.lock().unwrap().entries.insert(
            (
                SERVICE.to_string(),
                "vnidrop/v1/pairing-eligibility/corrupt".to_string(),
            ),
            secret.clone(),
        );
        let store = AppleKeychainSecretStore::with_api(api);

        let error = store
            .get(&handle("vnidrop/v1/pairing-eligibility/corrupt"))
            .unwrap_err();

        assert!(matches!(&error, SecureSecretStoreError::Corrupted));
        assert!(!format!("{error:?}").contains(&data_encoding::HEXLOWER.encode(&secret)));
    }
}
