use std::{sync::mpsc, thread, time::Instant};

use super::*;

fn until(description: &str, condition: impl Fn() -> bool) {
    eprintln!("GTK workflow: {description}");
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        while context.pending() {
            context.iteration(false);
        }
        assert!(Instant::now() < deadline, "timed out: {description}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn button(
    widget: &impl IsA<gtk::Widget>,
    matches: &impl Fn(&gtk::Button) -> bool,
) -> Option<gtk::Button> {
    if let Some(button) = widget.as_ref().downcast_ref::<gtk::Button>() {
        if matches(button) {
            return Some(button.clone());
        }
    }
    let mut child = widget.as_ref().first_child();
    while let Some(widget) = child {
        if let Some(button) = button(&widget, matches) {
            return Some(button);
        }
        child = widget.next_sibling();
    }
    None
}

#[test]
fn native_draft_approval_receive_and_shutdown() {
    adw::init().expect("GTK display (use xvfb-run for headless testing)");
    let application = adw::Application::builder()
        .application_id("com.vnidrop.VniDrop.UITest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("sender");
    let output = root.path().join("output");
    std::fs::create_dir(&output).unwrap();
    let mut preferences = Preferences::defaults(output.clone(), "Sender".into());
    preferences.network.mode = vnidrop::CoreRelayMode::LocalOnly;
    preferences.save(&profile).unwrap();
    let app = App::new(&application, profile);
    app.window.present();
    app.initialize();
    until("initial snapshot", || app.snapshot.borrow().is_some());
    assert_eq!(
        app.object::<gtk::Stack>("root")
            .visible_child_name()
            .as_deref(),
        Some("welcome")
    );

    let source = root.path().join("résumé.txt");
    std::fs::write(&source, b"GTK approval verified bytes").unwrap();
    app.compose(vec![gio::File::for_path(&source)]);
    until("send draft", || app.window.visible_dialog().is_some());
    let draft = app.window.visible_dialog().unwrap();
    let waiting = Instant::now();
    let reported = Cell::new(false);
    button(&draft, &|button| {
        button.label().as_deref() == Some(&text("linux_create_invitation"))
    })
    .unwrap()
    .emit_clicked();
    until("published invitation", || {
        if waiting.elapsed() > Duration::from_secs(5) && !reported.replace(true) {
            eprintln!(
                "GTK publication state: busy={}, dialog={:?}, transfers={:?}",
                app.busy.get(),
                app.window.visible_dialog().map(|d| d.title()),
                app.snapshot.borrow().as_ref().map(|s| s
                    .transfers
                    .iter()
                    .map(|t| &t.status)
                    .collect::<Vec<_>>())
            );
        }
        app.snapshot
            .borrow()
            .as_ref()
            .is_some_and(|s| s.transfers.iter().any(|t| t.status == "sharing"))
            && app.window.visible_dialog().is_none()
    });
    let transfer = app.snapshot.borrow().as_ref().unwrap().transfers[0].clone();
    assert_eq!(
        app.selected.borrow().as_ref(),
        Some(&(transfer.transfer_id, "send".into()))
    );

    let receiver_profile = root.path().join("receiver");
    let destination = output.clone();
    let (done, completion) = mpsc::channel();
    thread::spawn(move || {
        let receiver = Session::open(
            receiver_profile.to_str().unwrap().into(),
            vnidrop::CoreNetworkConfig {
                mode: vnidrop::CoreRelayMode::LocalOnly,
                relay_urls: vec![],
            },
        )
        .unwrap();
        let result = receiver.call(|core| {
            core.receive(
                transfer.ticket.unwrap(),
                destination.to_str().unwrap().into(),
                Some("Receiver".into()),
            )
        });
        receiver.close();
        done.send(result).unwrap();
    });
    app.selected.replace(None);
    app.render_details();
    until("global approval banner", || {
        app.object::<adw::Banner>("requests").is_revealed()
    });
    application.activate_action("review-requests", None);
    assert_eq!(
        app.selected.borrow().as_ref(),
        Some(&(transfer.transfer_id, "send".into()))
    );
    button(&app.details, &|button| {
        button.tooltip_text().as_deref() == Some(&text("button_approve"))
    })
    .unwrap()
    .emit_clicked();
    let received = RefCell::new(None);
    until("receiver completion", || {
        if let Ok(result) = completion.try_recv() {
            received.replace(Some(result));
        }
        received.borrow().is_some()
    });
    received.into_inner().unwrap().unwrap();
    assert_eq!(
        std::fs::read(output.join("résumé.txt")).unwrap(),
        b"GTK approval verified bytes"
    );
    until("approval banner dismissed", || {
        !app.object::<adw::Banner>("requests").is_revealed()
    });

    app.window.set_default_size(390, 700);
    until("narrow navigation", || app.split.is_collapsed());
    application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceDark);
    assert!(application.style_manager().is_dark());
    app.dispatch(
        move |session| session.call(|core| core.cancel_transfer(transfer.transfer_id)),
        |app, result| {
            result.unwrap();
            app.refresh();
        },
    );
    until("share stopped", || {
        app.busy.get() == 0
            && app
                .snapshot
                .borrow()
                .as_ref()
                .is_some_and(|s| !s.has_obligations())
    });
    app.request_close();
    until("window closed", || !app.window.is_visible());
    assert!(app.session.borrow().is_none());
}
