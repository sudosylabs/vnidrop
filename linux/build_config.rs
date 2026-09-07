use std::collections::BTreeMap;

pub fn properties(source: &str) -> BTreeMap<String, String> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with(['#', '!']) {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

pub fn diagnostics(
    project: &str,
    user: &str,
    endpoint: Option<String>,
    key: Option<String>,
) -> (String, String) {
    // Explicit native configuration is a pair: never mix it with a different deployment's key.
    if endpoint.is_some() || key.is_some() {
        return (endpoint.unwrap_or_default(), key.unwrap_or_default());
    }
    let mut values = properties(project);
    values.extend(properties(user));
    (
        values
            .remove("vnidrop.diagnostics.endpoint")
            .unwrap_or_default(),
        values
            .remove("vnidrop.diagnostics.ingestKey")
            .unwrap_or_default(),
    )
}

pub fn validate_delivery(endpoint: &str, key: &str, required: bool) -> Result<(), &'static str> {
    if endpoint.is_empty() && key.is_empty() {
        return if required {
            Err("Native release builds require a diagnostics endpoint and ingest key.")
        } else {
            Ok(())
        };
    }
    let url = url::Url::parse(endpoint).map_err(|_| "Invalid diagnostics endpoint.")?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if key.trim().is_empty()
        || key.len() > 4096
        || key.chars().any(char::is_control)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (url.scheme() != "https" && !(url.scheme() == "http" && loopback && !required))
        || (required && loopback)
    {
        return Err(
            "Diagnostics configuration must provide an HTTPS service and a valid ingest key.",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_reporting_requires_complete_public_https_configuration() {
        assert!(validate_delivery("", "", false).is_ok());
        assert!(validate_delivery("", "", true).is_err());
        assert!(validate_delivery("https://example.test", "", true).is_err());
        assert!(validate_delivery("http://localhost", "fixture", true).is_err());
        assert!(validate_delivery("http://localhost", "fixture", false).is_ok());
        assert!(validate_delivery("https://example.test", "fixture", true).is_ok());
        assert!(validate_delivery("https://user:pass@example.test", "fixture", true).is_err());
        assert!(validate_delivery("https://example.test", "fixture\n", true).is_err());
    }
    #[test]
    fn native_diagnostics_reuses_gradle_values_and_preserves_explicit_override() {
        let project = "vnidrop.diagnostics.endpoint=\nvnidrop.diagnostics.ingestKey=\n";
        let user = "# private user configuration\nvnidrop.diagnostics.endpoint=https://example.test\nvnidrop.diagnostics.ingestKey=fixture=key\n";
        assert_eq!(
            diagnostics(project, user, None, None),
            ("https://example.test".into(), "fixture=key".into())
        );
        assert_eq!(
            diagnostics(project, user, Some("https://other.test".into()), None),
            ("https://other.test".into(), String::new())
        );
        assert_eq!(
            diagnostics(project, user, Some(String::new()), Some(String::new())),
            (String::new(), String::new())
        );
    }
}
