use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::secure_secret::{
    apple::{
        expected_policy_for_test, handle_for_test, map_status_for_test, service_for_test,
        AppleKeychainApi, AppleKeychainPolicy, AppleKeychainSecretStore,
    },
    SecretMaterial, SecureSecretStore, SecureSecretStoreError,
};

const ERR_SEC_AUTH_FAILED: i32 = -25_293;
const ERR_SEC_NOT_AVAILABLE: i32 = -25_291;
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25_300;
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25_308;
const ERR_SEC_DECODE: i32 = -26_275;

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

#[test]
fn adapter_creates_replaces_reads_lists_and_deletes_only_its_service() {
    let api = RecordingKeychain::default();
    api.state.lock().unwrap().entries.insert(
        ("com.example.unrelated".to_string(), "leave-me".to_string()),
        vec![0x77; 32],
    );
    let store = AppleKeychainSecretStore::with_api(api.clone());
    let owned = handle_for_test("vnidrop/v1/endpoint-identity/apple-test");

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
            &handle_for_test("vnidrop/v1/relationship-grant/apple-policy"),
            SecretMaterial::new(vec![0x51; 32]).unwrap(),
        )
        .unwrap();

    assert_eq!(
        api.state.lock().unwrap().last_policy,
        Some(expected_policy_for_test())
    );
}

#[test]
fn apple_statuses_map_to_fail_closed_contract_outcomes() {
    assert!(matches!(
        map_status_for_test(ERR_SEC_INTERACTION_NOT_ALLOWED),
        SecureSecretStoreError::Locked
    ));
    assert!(matches!(
        map_status_for_test(ERR_SEC_AUTH_FAILED),
        SecureSecretStoreError::Locked
    ));
    assert!(matches!(
        map_status_for_test(ERR_SEC_ITEM_NOT_FOUND),
        SecureSecretStoreError::Missing
    ));
    assert!(matches!(
        map_status_for_test(ERR_SEC_DECODE),
        SecureSecretStoreError::Corrupted
    ));
    assert!(matches!(
        map_status_for_test(ERR_SEC_NOT_AVAILABLE),
        SecureSecretStoreError::Unavailable
    ));
    assert!(matches!(
        map_status_for_test(-1),
        SecureSecretStoreError::Unavailable
    ));
}

#[test]
fn malformed_keychain_values_are_corrupted_without_diagnostic_disclosure() {
    let api = RecordingKeychain::default();
    let secret = vec![0x6d; 31];
    api.state.lock().unwrap().entries.insert(
        (
            service_for_test().to_string(),
            "vnidrop/v1/pairing-eligibility/corrupt".to_string(),
        ),
        secret.clone(),
    );
    let store = AppleKeychainSecretStore::with_api(api);

    let error = store
        .get(&handle_for_test("vnidrop/v1/pairing-eligibility/corrupt"))
        .unwrap_err();

    assert!(matches!(&error, SecureSecretStoreError::Corrupted));
    assert!(!format!("{error:?}").contains(&data_encoding::HEXLOWER.encode(&secret)));
}
