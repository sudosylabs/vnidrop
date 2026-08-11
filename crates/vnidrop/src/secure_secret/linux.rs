use std::{collections::HashMap, sync::Arc};

use secret_service::{blocking::SecretService, EncryptionType, Error};

use super::{
    SecretHandle, SecretMaterial, SecureSecretStore, SecureSecretStoreError, HANDLE_NAMESPACE,
    HANDLE_VERSION,
};

const ATTRIBUTE_APPLICATION: &str = "application";
const ATTRIBUTE_HANDLE: &str = "vnidrop-handle";
const APPLICATION_ID: &str = "com.vnidrop.VniDrop";
const ITEM_LABEL: &str = "VniDrop protected secret";

pub(crate) trait LinuxSecretServiceApi: Send + Sync {
    fn put(&self, handle: &str, material: &[u8]) -> Result<(), SecureSecretStoreError>;
    fn get(&self, handle: &str) -> Result<Vec<u8>, SecureSecretStoreError>;
    fn delete(&self, handle: &str) -> Result<(), SecureSecretStoreError>;
    fn list_handles(&self) -> Result<Vec<String>, SecureSecretStoreError>;
}

struct SystemLinuxSecretService;

impl SystemLinuxSecretService {
    fn connect() -> Result<Self, SecureSecretStoreError> {
        SecretService::connect(EncryptionType::Dh).map_err(map_error)?;
        Ok(Self)
    }

    fn service(&self) -> Result<SecretService<'_>, SecureSecretStoreError> {
        SecretService::connect(EncryptionType::Dh).map_err(map_error)
    }
}

impl LinuxSecretServiceApi for SystemLinuxSecretService {
    fn put(&self, handle: &str, material: &[u8]) -> Result<(), SecureSecretStoreError> {
        let service = self.service()?;
        let collection = service.get_default_collection().map_err(map_error)?;
        if collection.is_locked().map_err(map_error)? {
            return Err(SecureSecretStoreError::Locked);
        }
        collection
            .create_item(
                ITEM_LABEL,
                HashMap::from([
                    (ATTRIBUTE_APPLICATION, APPLICATION_ID),
                    (ATTRIBUTE_HANDLE, handle),
                ]),
                material,
                true,
                "application/octet-stream",
            )
            .map_err(map_error)?;
        Ok(())
    }

    fn get(&self, handle: &str) -> Result<Vec<u8>, SecureSecretStoreError> {
        let service = self.service()?;
        let result = service
            .search_items(HashMap::from([
                (ATTRIBUTE_APPLICATION, APPLICATION_ID),
                (ATTRIBUTE_HANDLE, handle),
            ]))
            .map_err(map_error)?;
        if !result.locked.is_empty() {
            return Err(SecureSecretStoreError::Locked);
        }
        let mut items = result.unlocked.into_iter();
        let item = items.next().ok_or(SecureSecretStoreError::Missing)?;
        if items.next().is_some() {
            return Err(SecureSecretStoreError::Corrupted);
        }
        item.get_secret().map_err(map_error)
    }

    fn delete(&self, handle: &str) -> Result<(), SecureSecretStoreError> {
        let service = self.service()?;
        let result = service
            .search_items(HashMap::from([
                (ATTRIBUTE_APPLICATION, APPLICATION_ID),
                (ATTRIBUTE_HANDLE, handle),
            ]))
            .map_err(map_error)?;
        if !result.locked.is_empty() {
            return Err(SecureSecretStoreError::Locked);
        }
        if result.unlocked.is_empty() {
            return Err(SecureSecretStoreError::Missing);
        }
        for item in result.unlocked {
            item.delete().map_err(map_error)?;
        }
        Ok(())
    }

    fn list_handles(&self) -> Result<Vec<String>, SecureSecretStoreError> {
        let service = self.service()?;
        let result = service
            .search_items(HashMap::from([(ATTRIBUTE_APPLICATION, APPLICATION_ID)]))
            .map_err(map_error)?;
        if !result.locked.is_empty() {
            return Err(SecureSecretStoreError::Locked);
        }
        result
            .unlocked
            .into_iter()
            .map(|item| {
                item.get_attributes()
                    .map_err(map_error)?
                    .remove(ATTRIBUTE_HANDLE)
                    .ok_or(SecureSecretStoreError::Corrupted)
            })
            .collect()
    }
}

pub(crate) struct LinuxSecretServiceStore {
    api: Arc<dyn LinuxSecretServiceApi>,
}

impl LinuxSecretServiceStore {
    pub(crate) fn connect() -> Result<Self, SecureSecretStoreError> {
        Ok(Self {
            api: Arc::new(SystemLinuxSecretService::connect()?),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_api(api: Arc<dyn LinuxSecretServiceApi>) -> Self {
        Self { api }
    }
}

impl SecureSecretStore for LinuxSecretServiceStore {
    fn put(
        &self,
        handle: &SecretHandle,
        material: SecretMaterial,
    ) -> Result<(), SecureSecretStoreError> {
        self.api.put(handle.as_str(), &material.0)
    }

    fn get(&self, handle: &SecretHandle) -> Result<SecretMaterial, SecureSecretStoreError> {
        let bytes = self.api.get(handle.as_str())?;
        SecretMaterial::new(bytes).map_err(|_| SecureSecretStoreError::Corrupted)
    }

    fn delete(&self, handle: &SecretHandle) -> Result<(), SecureSecretStoreError> {
        self.api.delete(handle.as_str())
    }

    fn list_handles(&self) -> Result<Vec<SecretHandle>, SecureSecretStoreError> {
        let expected_prefix = format!("{HANDLE_NAMESPACE}/{HANDLE_VERSION}/");
        let mut handles = self
            .api
            .list_handles()?
            .into_iter()
            .map(|handle| {
                if handle.starts_with(&expected_prefix) {
                    Ok(SecretHandle(handle))
                } else {
                    Err(SecureSecretStoreError::Corrupted)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        handles.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        Ok(handles)
    }
}

pub(crate) fn map_error(error: Error) -> SecureSecretStoreError {
    match error {
        Error::Locked | Error::Prompt => SecureSecretStoreError::Locked,
        Error::NoResult => SecureSecretStoreError::Missing,
        Error::Crypto(_) => SecureSecretStoreError::Corrupted,
        Error::Unavailable | Error::Zvariant(_) | Error::Zbus(_) | Error::ZbusFdo(_) => {
            SecureSecretStoreError::Unavailable
        }
        _ => SecureSecretStoreError::Unavailable,
    }
}
