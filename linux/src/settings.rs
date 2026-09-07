use crate::error::Result;
use std::{collections::HashSet, path::Path};
use vnidrop::{CoreNetworkConfig, CoreRelayMode};

pub fn network(mode: CoreRelayMode, input: &str, retained: &[String]) -> Result<CoreNetworkConfig> {
    if matches!(mode, CoreRelayMode::Automatic | CoreRelayMode::LocalOnly) {
        return Ok(CoreNetworkConfig {
            mode,
            relay_urls: retained.to_vec(),
        });
    }
    let lines: Vec<_> = input
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if lines.is_empty() {
        return Err("relay_validation_missing_url");
    }
    if lines.len() > 8 {
        return Err("error_invalid_input");
    }
    let mut seen = HashSet::new();
    let mut relay_urls = Vec::new();
    for line in lines {
        if line.len() > 2048 || line.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("error_invalid_input");
        }
        let url = url::Url::parse(line).map_err(|_| "error_invalid_input")?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || url.port() == Some(0)
            || !seen.insert(url.clone())
        {
            return Err("error_invalid_input");
        }
        relay_urls.push(url.to_string());
    }
    Ok(CoreNetworkConfig { mode, relay_urls })
}

pub fn validate_folder(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("error_filesystem");
    }
    std::fs::create_dir_all(path).map_err(|_| "error_filesystem")?;
    let probe = path.join(format!(".vnidrop-access-{}", uuid::Uuid::new_v4()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| "error_filesystem")?;
    std::fs::remove_file(probe).map_err(|_| "error_filesystem")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_rejects_unsafe_or_duplicate_urls_and_retains_inactive_input() {
        for value in [
            "",
            "http://relay.example",
            "https://user@relay.example",
            "https://relay.example/path",
            "https://relay.example?q=a",
            "https://relay.example#x",
            "https://relay.example:0",
            "https://relay.example\nhttps://RELAY.example/",
        ] {
            assert!(
                network(CoreRelayMode::StrictCustom, value, &[]).is_err(),
                "{value}"
            );
        }
        assert!(network(
            CoreRelayMode::StrictCustom,
            &["https://relay.example"; 9].join("\n"),
            &[]
        )
        .is_err());
        let retained = vec!["https://relay.example".into()];
        assert_eq!(
            network(CoreRelayMode::LocalOnly, "invalid", &retained)
                .unwrap()
                .relay_urls,
            retained
        );
        assert_eq!(
            network(
                CoreRelayMode::StrictCustom,
                " https://RELAY.example \n",
                &[]
            )
            .unwrap()
            .relay_urls,
            ["https://relay.example/"]
        );
    }
    #[test]
    fn folder_probe_preserves_existing_files_and_rejects_a_file_destination() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("keep");
        std::fs::write(&file, b"keep").unwrap();
        assert!(validate_folder(&file).is_err());
        validate_folder(root.path()).unwrap();
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }
}
