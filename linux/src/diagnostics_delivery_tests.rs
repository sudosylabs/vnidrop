use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    thread,
    time::Instant,
};

fn service(statuses: Vec<u16>) -> (Configuration, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        for status in statuses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "diagnostics request never arrived"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut headers = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(!line.is_empty());
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                headers.push_str(&line.to_ascii_lowercase());
            }
            assert!(headers.starts_with("post /v1/bugs "));
            assert!(headers.contains("x-vnidrop-key: fixture-key"));
            assert!(headers.contains("user-agent: vnidrop/"));
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let request: Value = serde_json::from_slice(&body).unwrap();
            let response = json!({"ok":true,"id":request["id"]}).to_string();
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            requests.push(request);
        }
        requests
    });
    (
        Configuration::new(format!("http://{address}"), "fixture-key".into()).unwrap(),
        worker,
    )
}
fn draft() -> Draft {
    Draft {
        what: "Fixture failure".into(),
        expected: "Fixture success".into(),
        ..Default::default()
    }
}
#[test]
fn server_failures_have_actionable_feedback() {
    for (status, expected) in [
        (401, "linux_report_service_configuration"),
        (403, "linux_report_service_configuration"),
        (404, "linux_report_service_configuration"),
        (413, "linux_report_invalid_payload"),
        (429, "linux_report_rate_limited"),
        (500, "linux_report_server_error"),
        (503, "linux_report_server_error"),
        (302, "linux_report_unconfirmed"),
    ] {
        let (configuration, worker) = service(vec![status]);
        let report = assemble(&draft(), "fixture", "test", "LocalOnly", &[]).unwrap();
        assert_eq!(configuration.send(&report), Err(expected));
        worker.join().unwrap();
    }
}
#[test]
fn failed_submission_retains_payload_and_reference_until_edited_or_cleared() {
    let (configuration, worker) = service(vec![503, 202, 202]);
    let mut submission = Submission {
        draft: draft(),
        ..Default::default()
    };
    let first = submission
        .prepare("fixture", "test", "LocalOnly", &[])
        .unwrap();
    assert_eq!(configuration.send(&first), Err("linux_report_server_error"));
    let retry = submission
        .prepare("fixture", "changed device name", "Automatic", &[])
        .unwrap();
    assert_eq!(first, retry);
    assert_eq!(
        configuration.send(&retry).unwrap(),
        first["id"].as_str().unwrap()
    );
    submission.draft.what = "A different failure".into();
    let edited = submission
        .prepare("fixture", "test", "LocalOnly", &[])
        .unwrap();
    assert_ne!(edited["id"], first["id"]);
    configuration.send(&edited).unwrap();
    assert_eq!(worker.join().unwrap(), vec![first, retry, edited]);
    submission.clear();
    assert!(submission.draft == Draft::default());
    assert!(submission.cached.is_none());
}
#[test]
fn description_limits_match_server_utf8_limits_without_silent_truncation() {
    let mut draft = draft();
    draft.what = "é".repeat(2000);
    assert!(assemble(&draft, "fixture", "test", "LocalOnly", &[]).is_ok());
    draft.what.push('a');
    assert_eq!(
        assemble(&draft, "fixture", "test", "LocalOnly", &[]),
        Err("linux_report_text_too_long")
    );
}
#[test]
fn connection_failure_does_not_claim_delivery() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let configuration = Configuration::new(format!("http://{address}"), "fixture".into()).unwrap();
    assert_eq!(
        configuration.send(&json!({"id":"fixture"})),
        Err("linux_report_connection_failed")
    );
}
