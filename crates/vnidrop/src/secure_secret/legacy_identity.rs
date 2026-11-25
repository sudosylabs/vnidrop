use std::{io, path::Path};

use data_encoding::HEXLOWER;

use super::{SecretMaterial, VnidropError};

pub(super) async fn remove_legacy_identity_durably(path: &Path) -> Result<(), VnidropError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(VnidropError::filesystem(error)),
        }
        #[cfg(unix)]
        {
            let parent = path.parent().ok_or_else(|| {
                VnidropError::filesystem(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "legacy identity path has no parent",
                ))
            })?;
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(VnidropError::filesystem)?;
        }
        Ok(())
    })
    .await
    .map_err(VnidropError::internal)?
}

pub(super) async fn read_legacy_endpoint_identity(
    path: &Path,
) -> Result<SecretMaterial, VnidropError> {
    let encoded = match tokio::fs::read_to_string(path).await {
        Ok(encoded) => encoded,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(VnidropError::SecureStorageMissing {
                reason: "no protected or legacy endpoint identity exists".to_string(),
            });
        }
        Err(error) => return Err(VnidropError::filesystem(error)),
    };
    let bytes = HEXLOWER.decode(encoded.trim().as_bytes()).map_err(|_| {
        VnidropError::SecureStorageCorrupted {
            reason: "legacy endpoint key encoding is invalid".to_string(),
        }
    })?;
    SecretMaterial::new(bytes)
}
