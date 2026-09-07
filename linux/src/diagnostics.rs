include!(concat!(env!("OUT_DIR"), "/diagnostics_config.rs"));
#[cfg(test)]
#[path = "../build_config.rs"]
mod build_config;
use crate::error::Result;
use serde_json::{json, Value};
use std::{
    io::Read,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub what: String,
    pub expected: String,
    pub steps: String,
    pub contact: String,
    pub include_logs: bool,
}
#[derive(Default)]
pub struct Submission {
    pub draft: Draft,
    cached: Option<(Draft, Value)>,
}
impl Submission {
    pub fn prepare(
        &mut self,
        install_id: &str,
        name: &str,
        network: &str,
        events: &[vnidrop::CoreEvent],
    ) -> Result<Value> {
        if let Some((draft, report)) = &self.cached {
            if draft == &self.draft {
                return Ok(report.clone());
            }
        }
        let report = assemble(&self.draft, install_id, name, network, events)?;
        self.cached = Some((self.draft.clone(), report.clone()));
        Ok(report)
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

pub fn build_configured() -> bool {
    Configuration::new(BUILD_ENDPOINT.into(), BUILD_KEY.into()).is_ok()
}
#[derive(Clone)]
pub struct Configuration {
    endpoint: String,
    key: String,
}
impl Configuration {
    pub fn configured() -> Result<Self> {
        let native_override = std::env::var_os("VNIDROP_DIAGNOSTICS_ENDPOINT").is_some()
            || std::env::var_os("VNIDROP_DIAGNOSTICS_INGEST_KEY").is_some();
        let endpoint = std::env::var("VNIDROP_DIAGNOSTICS_ENDPOINT")
            .ok()
            .or_else(|| (!native_override).then(|| BUILD_ENDPOINT.to_owned()))
            .unwrap_or_default();
        let key = std::env::var("VNIDROP_DIAGNOSTICS_INGEST_KEY")
            .ok()
            .or_else(|| (!native_override).then(|| BUILD_KEY.to_owned()))
            .unwrap_or_default();
        Self::new(endpoint, key)
    }
    pub fn new(endpoint: String, key: String) -> Result<Self> {
        let parsed = url::Url::parse(&endpoint).map_err(|_| "linux_diagnostics_unconfigured")?;
        let loopback = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if key.trim().is_empty()
            || (!loopback && parsed.scheme() != "https")
            || !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err("linux_diagnostics_unconfigured");
        }
        Ok(Self {
            endpoint: endpoint.trim_end_matches('/').into(),
            key,
        })
    }
    pub fn send(&self, report: &Value) -> Result<String> {
        let bytes = serde_json::to_vec(report).map_err(|_| "bug_report_submit_failed")?;
        if bytes.len() > 256 * 1024 {
            return Err("error_invalid_input");
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("VniDrop/Linux-native ", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "bug_report_submit_failed")?;
        let response = client
            .post(format!("{}/v1/bugs", self.endpoint))
            .header("X-VniDrop-Key", &self.key)
            .header(
                "X-VniDrop-Install-Id",
                report["installId"].as_str().unwrap_or_default(),
            )
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .body(bytes)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    "linux_report_timeout"
                } else if error.is_connect() {
                    "linux_report_connection_failed"
                } else {
                    "linux_report_unconfirmed"
                }
            })?;
        match response.status().as_u16() {
            200..=299 => {}
            401 | 403 | 404 => return Err("linux_report_service_configuration"),
            429 => return Err("linux_report_rate_limited"),
            500..=599 => return Err("linux_report_server_error"),
            400 | 413 | 415 | 422 => return Err("linux_report_invalid_payload"),
            _ => return Err("linux_report_unconfirmed"),
        }
        let mut body = Vec::new();
        response
            .take(8193)
            .read_to_end(&mut body)
            .map_err(|_| "linux_report_unconfirmed")?;
        if body.len() > 8192 {
            return Err("linux_report_unconfirmed");
        }
        #[derive(serde::Deserialize)]
        struct Acknowledgement {
            ok: bool,
            id: String,
        }
        let ack: Acknowledgement =
            serde_json::from_slice(&body).map_err(|_| "linux_report_unconfirmed")?;
        if !ack.ok || Some(ack.id.as_str()) != report["id"].as_str() {
            return Err("linux_report_unconfirmed");
        }
        Ok(ack.id)
    }
}
pub fn version() -> &'static str {
    include_str!("../../version.properties")
        .lines()
        .find_map(|line| line.strip_prefix("PRODUCT_VERSION="))
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}
pub fn assemble(
    draft: &Draft,
    install_id: &str,
    name: &str,
    network: &str,
    events: &[vnidrop::CoreEvent],
) -> Result<Value> {
    if draft.what.trim().is_empty() {
        return Err("bug_report_missing_what");
    }
    if draft.expected.trim().is_empty() {
        return Err("bug_report_missing_expected");
    }
    if [&draft.what, &draft.expected, &draft.steps]
        .iter()
        .any(|s| s.len() > 4000)
        || draft.contact.len() > 320
    {
        return Err("linux_report_text_too_long");
    }
    let safe = |value: &str| {
        value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(64)
            .collect::<String>()
    };
    let logs = if draft.include_logs {
        events
            .iter()
            .take(100)
            .map(|e| format!("{} {} {}", e.timestamp, safe(&e.phase), safe(&e.kind)))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };
    Ok(
        json!({"schemaVersion":1,"id":uuid::Uuid::new_v4().to_string(),"timestampMillis":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64,"installId":install_id,"appVersion":version(),"platform":"linux-native","whatHappened":draft.what.trim(),"expected":draft.expected.trim(),"steps":draft.steps.trim(),"contact":draft.contact.trim(),"includeLogs":draft.include_logs,"logs":logs,"device":{"deviceName":name,"deviceModel":std::env::consts::ARCH,"operatingSystem":"Linux","network":network,"batteryLevel":""}}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_configuration_and_required_fields() {
        assert!(Configuration::new("http://example.com".into(), "key".into()).is_err());
        assert!(Configuration::new("https://example.com".into(), String::new()).is_err());
        assert!(assemble(&Draft::default(), "id", "device", "LocalOnly", &[]).is_err());
    }
    #[test]
    fn transport_requires_matching_acknowledgement() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            thread,
        };
        for success in [true, false] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let report = assemble(
                &Draft {
                    what: "failed".into(),
                    expected: "worked".into(),
                    ..Default::default()
                },
                "fixture",
                "test",
                "LocalOnly",
                &[],
            )
            .unwrap();
            let expected = report["id"].clone();
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0;
                let mut line = String::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(request["whatHappened"], "failed");
                let body =
                    json!({"ok":true,"id":if success{expected}else{json!("wrong")}}).to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            });
            assert_eq!(
                Configuration::new(format!("http://{address}"), "fixture-key".into())
                    .unwrap()
                    .send(&report)
                    .is_ok(),
                success
            );
            worker.join().unwrap();
        }
    }
}

#[cfg(test)]
mod privacy_tests {
    use super::*;
    #[test]
    fn report_activity_never_attaches_event_payloads_or_endpoint_identifiers() {
        let event=vnidrop::CoreEvent{id:"sensitive-id".into(),revision:1,timestamp:1,scope:"transfer".into(),transfer_id:Some(1),direction:Some("send".into()),phase:"download".into(),kind:"progress".into(),data_json:json!({"ticket":"sensitive-ticket","path":"/private/file","endpoint_id":"sensitive-peer"}).to_string()};
        let draft = Draft {
            what: "Failure".into(),
            expected: "Success".into(),
            include_logs: true,
            ..Default::default()
        };
        let report = assemble(&draft, "fixture", "test", "LocalOnly", &[event]).unwrap();
        assert_eq!(report["logs"], "1 download progress");
        for secret in [
            "sensitive-id",
            "sensitive-ticket",
            "/private/file",
            "sensitive-peer",
        ] {
            assert!(!report.to_string().contains(secret));
        }
    }
}

#[cfg(test)]
#[path = "diagnostics_delivery_tests.rs"]
mod delivery_tests;
