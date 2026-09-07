use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use vnidrop_gnome::diagnostics::Configuration;

fn labels(widget: &gtk::Widget, expected: &str) -> bool {
    if widget
        .downcast_ref::<gtk::Label>()
        .is_some_and(|l| l.text().contains(expected))
    {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if labels(&widget, expected) {
            return true;
        }
        child = widget.next_sibling();
    }
    false
}
fn editors(widget: &gtk::Widget, output: &mut Vec<gtk::TextView>) {
    if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
        output.push(view.clone());
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        editors(&widget, output);
        child = widget.next_sibling();
    }
}

pub(super) fn report_failure_retry_and_receipt(root: &std::path::Path) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let mut reports = Vec::new();
        for status in [503, 202] {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "report was not submitted");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("report fixture: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
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
            }
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let response = serde_json::json!({"ok":true,"id":report["id"]}).to_string();
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            reports.push(report);
        }
        reports
    });
    let configuration =
        Configuration::new(format!("http://{address}"), "fixture-key".into()).unwrap();
    let application = adw::Application::builder()
        .application_id("com.vnidrop.VniDrop.ReportUITest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let app = App::new(&application, root.join("report-profile"));
    app.window.set_default_size(390, 700);
    app.window.present();
    app.present_bug_report(Ok(configuration.clone()));
    let dialog = app.window.visible_dialog().unwrap();
    let mut fields = Vec::new();
    editors(dialog.upcast_ref(), &mut fields);
    assert_eq!(fields.len(), 3);
    fields[0]
        .buffer()
        .set_text("Local GTK test: report failure");
    fields[1].buffer().set_text("A confirmed report receipt");
    entry(&dialog, &text("bug_report_contact_label"))
        .unwrap()
        .set_text("fixture@example.test");
    render_window(&app.window);
    activate_button(&dialog, "bug_report_submit");
    until("report failure is visible", || {
        labels(dialog.upcast_ref(), &text("linux_report_server_error"))
    });
    assert!(dialog.can_close());
    assert!(!labels(dialog.upcast_ref(), &text("bug_report_submitted")));
    capture(&app.window, "report-failure-narrow");
    dialog.close();
    until("failed report closed", || {
        app.window.visible_dialog().is_none()
    });
    app.present_bug_report(Ok(configuration));
    let dialog = app.window.visible_dialog().unwrap();
    let mut reopened = Vec::new();
    editors(dialog.upcast_ref(), &mut reopened);
    assert_eq!(
        reopened[0].buffer().text(
            &reopened[0].buffer().start_iter(),
            &reopened[0].buffer().end_iter(),
            false
        ),
        "Local GTK test: report failure"
    );
    assert_eq!(
        entry(&dialog, &text("bug_report_contact_label"))
            .unwrap()
            .text(),
        "fixture@example.test"
    );
    render_window(&app.window);
    activate_button(&dialog, "bug_report_submit");
    until("report receipt visible", || {
        labels(dialog.upcast_ref(), &text("bug_report_submitted"))
    });
    let reports = worker.join().unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0], reports[1]);
    assert!(labels(
        dialog.upcast_ref(),
        reports[0]["id"].as_str().unwrap()
    ));
    assert!(app.report.borrow().draft == Default::default());
    capture(&app.window, "report-receipt-narrow");
    activate_button(&dialog, "button_close");
    until("receipt closed", || app.window.visible_dialog().is_none());
    app.request_close();
    until("report window closed", || !app.window.is_visible());
}
