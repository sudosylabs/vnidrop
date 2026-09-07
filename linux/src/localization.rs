use serde_json::Value;
use std::sync::OnceLock;
fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../data/strings.json"))
            .expect("generated Linux localization")
    })
}
pub fn language() -> &'static str {
    static LANGUAGE: OnceLock<String> = OnceLock::new();
    LANGUAGE.get_or_init(|| {
        for name in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(name) {
                for candidate in value.split(':') {
                    let language = candidate
                        .split(['_', '-', '.'])
                        .next()
                        .unwrap_or("en")
                        .to_lowercase();
                    if catalog()["languages"].get(&language).is_some() {
                        return language;
                    }
                }
            }
        }
        "en".into()
    })
}
fn plural(language: &str, n: u64) -> &'static str {
    match language {
        "ru" if n % 10 == 1 && n % 100 != 11 => "one",
        "ru" if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) => "few",
        "ru" => "many",
        "pl" if n == 1 => "one",
        "pl" if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) => "few",
        "pl" => "many",
        "fr" | "pt" if n <= 1 => "one",
        "fr" | "pt" | "es" | "it" if n != 0 && n.is_multiple_of(1_000_000) => "many",
        _ if n == 1 => "one",
        _ => "other",
    }
}
pub fn text(key: &str) -> String {
    lookup(language(), key, None)
}
fn lookup(language: &str, key: &str, count: Option<u64>) -> String {
    let local = &catalog()["languages"][language][key];
    let source = &catalog()["languages"]["en"][key];
    let read = |value: &Value| {
        value
            .as_str()
            .or_else(|| value[count.map(|n| plural(language, n)).unwrap_or("other")].as_str())
            .or_else(|| value["other"].as_str())
            .map(str::to_owned)
    };
    read(local)
        .or_else(|| read(source))
        .unwrap_or_else(|| key.to_owned())
}
pub fn format(key: &str, arguments: &[(&str, &str)]) -> String {
    let count = arguments
        .iter()
        .find(|(key, _)| *key == "count")
        .and_then(|(_, value)| value.parse().ok());
    interpolate(&lookup(language(), key, count), arguments)
}
fn interpolate(template: &str, arguments: &[(&str, &str)]) -> String {
    let mut result = String::new();
    let mut remaining = template;
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_referenced_keys_exist_in_generated_catalog() {
        fn check(directory: &std::path::Path, source: &Value) {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    check(&path, source);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let contents = std::fs::read_to_string(&path).unwrap();
                    for key in contents.split('"').skip(1).step_by(2) {
                        if source["strings"].get(key).is_some() {
                            assert!(
                                catalog()["languages"]["en"].get(key).is_some(),
                                "missing Linux translation: {key} in {}",
                                path.display()
                            );
                        }
                    }
                }
            }
        }
        let source: Value =
            serde_json::from_str(include_str!("../../localization/strings.json")).unwrap();
        check(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &source,
        );
    }
    #[test]
    fn plural_rules_handle_slavic_teens_and_french_zero() {
        assert_eq!(
            [1, 2, 5, 11, 21, 22, 25, 111].map(|n| plural("ru", n)),
            ["one", "few", "many", "many", "one", "few", "many", "many"]
        );
        assert_eq!(
            [1, 2, 5, 12, 21, 22].map(|n| plural("pl", n)),
            ["one", "few", "many", "many", "many", "few"]
        );
        assert_eq!(plural("fr", 0), "one");
        assert_eq!(plural("en", 0), "other");
        assert_eq!(
            interpolate("{name}: {count}", &[("name", "{count}"), ("count", "2")]),
            "{count}: 2"
        );
        assert_eq!(
            lookup("unknown", "button_cancel", None),
            lookup("en", "button_cancel", None)
        );
    }
}
