use std::sync::OnceLock;

use serde_json::Value;

fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/strings.json"))
            .expect("generated Linux localization")
    })
}

pub fn text(key: &str) -> String {
    static LANGUAGE: OnceLock<String> = OnceLock::new();
    let language = LANGUAGE.get_or_init(|| {
        ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
            .unwrap_or_else(|| "en".into())
            .split([':', '_', '-', '.'])
            .next()
            .unwrap_or("en")
            .to_lowercase()
    });
    catalog()["languages"][language][key]
        .as_str()
        .or_else(|| catalog()["languages"]["en"][key].as_str())
        .unwrap_or(key)
        .to_owned()
}

pub fn format(key: &str, arguments: &[(&str, &str)]) -> String {
    let template = text(key);
    let mut result = String::new();
    let mut remaining = template.as_str();
    while let Some(start) = remaining.find('{') {
        let Some(end) = remaining[start..].find('}').map(|end| end + start) else {
            break;
        };
        result.push_str(&remaining[..start]);
        let name = &remaining[start + 1..end];
        result.push_str(
            arguments
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap_or(&remaining[start..=end]),
        );
        remaining = &remaining[end + 1..];
    }
    result.push_str(remaining);
    result
}
