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

#[cfg(test)]
mod tests {
    use super::*;
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
