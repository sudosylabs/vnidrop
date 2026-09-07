use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use vnidrop::{CoreNetworkConfig, CoreRelayMode};

use crate::error::Result;

const INVALID: &str = "linux_preferences_invalid";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preferences {
    pub username: String,
    pub receive_directory: PathBuf,
    pub theme: String,
    pub network: CoreNetworkConfig,
    pub notifications: bool,
    pub diagnostics_install_id: String,
}

impl Preferences {
    pub fn network_config(&self) -> CoreNetworkConfig {
        // Preferences retain custom URLs when their mode is inactive, as Compose does.
        let relay_urls = match self.network.mode {
            CoreRelayMode::Automatic | CoreRelayMode::LocalOnly => Vec::new(),
            CoreRelayMode::StrictCustom | CoreRelayMode::CustomWithDirectFallback => {
                self.network.relay_urls.clone()
            }
        };
        CoreNetworkConfig {
            mode: self.network.mode,
            relay_urls,
        }
    }

    pub fn defaults(downloads: PathBuf, username: String) -> Self {
        Self {
            username,
            receive_directory: downloads,
            theme: "System".into(),
            network: CoreNetworkConfig::default(),
            notifications: false,
            diagnostics_install_id: String::new(),
        }
    }

    pub fn load(profile: &Path, defaults: Self) -> Result<Self> {
        let native = profile.join("linux-preferences.json");
        if native.try_exists().map_err(|_| INVALID)? {
            let prefs: Self =
                serde_json::from_slice(&read_bounded(&native)?).map_err(|_| INVALID)?;
            return prefs.validate();
        }
        let legacy = profile.join("app_preferences.preferences_pb");
        if legacy.try_exists().map_err(|_| INVALID)? {
            return Self::import_legacy(&read_bounded(&legacy)?, defaults);
        }
        defaults.validate()
    }

    pub fn save(&self, profile: &Path) -> Result<()> {
        self.clone().validate()?;
        fs::create_dir_all(profile).map_err(|_| "error_filesystem")?;
        let temporary = profile.join(format!("linux-preferences.{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            fs::rename(&temporary, profile.join("linux-preferences.json"))?;
            fs::File::open(profile)?.sync_all()
        })();
        let _ = fs::remove_file(temporary);
        result.map_err(|_: std::io::Error| "error_filesystem")
    }

    fn validate(self) -> Result<Self> {
        if self.username.trim().is_empty()
            || !self.receive_directory.is_absolute()
            || !matches!(self.theme.as_str(), "System" | "Light" | "Dark")
        {
            return Err(INVALID);
        }
        // Network URL validation remains in the core. An invalid stored policy
        // must never fall back to public relay/discovery during migration.
        Ok(self)
    }

    fn import_legacy(bytes: &[u8], mut prefs: Self) -> Result<Self> {
        let mut values = HashMap::new();
        let mut map = Proto(bytes);
        while !map.0.is_empty() {
            let tag = map.varint()?;
            if tag != 10 {
                map.skip(tag)?;
                continue;
            }
            let mut entry = Proto(map.bytes()?);
            let mut key = None;
            let mut value = None;
            while !entry.0.is_empty() {
                match entry.varint()? {
                    10 => {
                        key = Some(
                            std::str::from_utf8(entry.bytes()?)
                                .map_err(|_| INVALID)?
                                .to_owned(),
                        )
                    }
                    18 => {
                        let mut item = Proto(entry.bytes()?);
                        while !item.0.is_empty() {
                            match item.varint()? {
                                42 => {
                                    value = Some(serde_json::Value::String(
                                        std::str::from_utf8(item.bytes()?)
                                            .map_err(|_| INVALID)?
                                            .to_owned(),
                                    ))
                                }
                                8 => value = Some(serde_json::Value::Bool(item.varint()? != 0)),
                                tag => item.skip(tag)?,
                            }
                        }
                    }
                    tag => entry.skip(tag)?,
                }
            }
            if let Some(key) = key {
                values.insert(key, value.ok_or(INVALID)?);
            }
        }
        let text = |key: &str| -> Result<Option<&str>> {
            values
                .get(key)
                .map(|v| v.as_str().ok_or(INVALID))
                .transpose()
        };
        if let Some(value) = text("username")? {
            prefs.username = value.into();
        }
        if let Some(value) = text("receive_folder_value")? {
            prefs.receive_directory = value.into();
        }
        if let Some(value) = text("receive_folder_kind")? {
            if value != "FileSystemPath" {
                return Err(INVALID);
            }
        }
        if let Some(value) = text("theme_mode")? {
            prefs.theme = value.into();
        }
        if let Some(value) = text("diagnostics_install_id")? {
            prefs.diagnostics_install_id = value.into();
        }
        if let Some(value) = text("relay_mode")? {
            prefs.network.mode = match value {
                "Automatic" => CoreRelayMode::Automatic,
                "StrictCustom" => CoreRelayMode::StrictCustom,
                "CustomWithDirectFallback" => CoreRelayMode::CustomWithDirectFallback,
                "LocalOnly" => CoreRelayMode::LocalOnly,
                _ => return Err(INVALID),
            };
        }
        if let Some(value) = text("relay_urls")? {
            prefs.network.relay_urls = value
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
        }
        if let Some(value) = values.get("notifications_enabled") {
            prefs.notifications = value.as_bool().ok_or(INVALID)?;
        }
        prefs.validate()
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|_| INVALID)?;
    if bytes.len() > 1024 * 1024 {
        return Err(INVALID);
    }
    Ok(bytes)
}

struct Proto<'a>(&'a [u8]);

impl<'a> Proto<'a> {
    fn varint(&mut self) -> Result<u64> {
        let mut result = 0;
        for shift in (0..64).step_by(7) {
            let (&byte, remaining) = self.0.split_first().ok_or(INVALID)?;
            self.0 = remaining;
            if shift == 63 && byte > 1 {
                return Err(INVALID);
            }
            result |= u64::from(byte & 127) << shift;
            if byte < 128 {
                return Ok(result);
            }
        }
        Err(INVALID)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        if length > self.0.len() {
            return Err(INVALID);
        }
        let (value, remaining) = self.0.split_at(length);
        self.0 = remaining;
        Ok(value)
    }

    fn bytes(&mut self) -> Result<&'a [u8]> {
        let size = usize::try_from(self.varint()?).map_err(|_| INVALID)?;
        self.take(size)
    }

    fn skip(&mut self, tag: u64) -> Result<()> {
        if tag >> 3 == 0 {
            return Err(INVALID);
        }
        match tag & 7 {
            0 => {
                self.varint()?;
            }
            1 => {
                self.take(8)?;
            }
            2 => {
                self.bytes()?;
            }
            5 => {
                self.take(4)?;
            }
            _ => return Err(INVALID),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Preferences {
        Preferences::defaults("/tmp/downloads".into(), "Laptop".into())
    }

    fn legacy(items: &[(&str, &str)]) -> Vec<u8> {
        let mut result = Vec::new();
        for (key, value) in items {
            let mut entry = vec![10, key.len() as u8];
            entry.extend(key.as_bytes());
            entry.extend([18, value.len() as u8 + 2, 42, value.len() as u8]);
            entry.extend(value.as_bytes());
            result.extend([10, entry.len() as u8]);
            result.extend(entry);
        }
        result
    }

    #[test]
    fn imports_legacy_profile_without_changing_network_policy_or_source() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = legacy(&[
            ("username", "Émilie"),
            ("theme_mode", "Dark"),
            ("relay_mode", "StrictCustom"),
            ("relay_urls", "https://relay.example"),
            ("receive_folder_value", "/tmp/réception"),
        ]);
        let path = dir.path().join("app_preferences.preferences_pb");
        fs::write(&path, &bytes).unwrap();
        let prefs = Preferences::load(dir.path(), defaults()).unwrap();
        assert_eq!(prefs.username, "Émilie");
        assert_eq!(prefs.network.mode, CoreRelayMode::StrictCustom);
        assert_eq!(prefs.network.relay_urls, ["https://relay.example"]);
        prefs.save(dir.path()).unwrap();
        assert_eq!(fs::read(path).unwrap(), bytes);
        let reopened = Preferences::load(dir.path(), defaults()).unwrap();
        assert_eq!(
            serde_json::to_value(reopened).unwrap(),
            serde_json::to_value(prefs).unwrap()
        );
    }

    #[test]
    fn imported_inactive_relay_urls_are_retained_but_not_passed_to_core() {
        for (mode, expected_mode) in [
            ("Automatic", CoreRelayMode::Automatic),
            ("LocalOnly", CoreRelayMode::LocalOnly),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let bytes = legacy(&[
                ("relay_mode", mode),
                ("relay_urls", "https://relay.example"),
            ]);
            fs::write(dir.path().join("app_preferences.preferences_pb"), bytes).unwrap();
            let prefs = Preferences::load(dir.path(), defaults()).unwrap();
            assert_eq!(
                prefs.network_config(),
                CoreNetworkConfig {
                    mode: expected_mode,
                    relay_urls: vec![]
                }
            );
            assert_eq!(prefs.network.relay_urls, ["https://relay.example"]);
            prefs.save(dir.path()).unwrap();
            let reopened = Preferences::load(dir.path(), defaults()).unwrap();
            assert_eq!(reopened.network, prefs.network);
            assert_eq!(reopened.network_config(), prefs.network_config());
        }
    }

    #[test]
    fn custom_relay_modes_pass_saved_urls_to_core() {
        for mode in ["StrictCustom", "CustomWithDirectFallback"] {
            let prefs = Preferences::import_legacy(
                &legacy(&[
                    ("relay_mode", mode),
                    ("relay_urls", "https://relay.example"),
                ]),
                defaults(),
            )
            .unwrap();
            assert_eq!(prefs.network_config(), prefs.network);
        }
    }

    #[test]
    fn malformed_preferences_fail_closed_instead_of_enabling_public_relays() {
        assert_eq!(
            Preferences::import_legacy(&legacy(&[("relay_mode", "unknown")]), defaults())
                .unwrap_err(),
            INVALID
        );
        assert_eq!(
            Preferences::import_legacy(&[10, 120], defaults()).unwrap_err(),
            INVALID
        );
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("linux-preferences.json"), "{}").unwrap();
        assert_eq!(
            Preferences::load(dir.path(), defaults()).unwrap_err(),
            INVALID
        );
    }
}
