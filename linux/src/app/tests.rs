use std::{sync::mpsc, thread, time::Instant};

use super::*;

#[path = "device_transfers_tests.rs"]
mod device_transfers_tests;
#[path = "navigation_tests.rs"]
mod navigation_tests;
#[path = "reporting_ui_tests.rs"]
mod reporting_ui_tests;
#[path = "settings_ui_tests.rs"]
mod settings_ui_tests;
#[path = "transfer_details_tests.rs"]
mod transfer_details_tests;

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

fn activate_button(widget: &impl IsA<gtk::Widget>, key: &str) {
    button(widget, &|button| {
        button.label().as_deref() == Some(&text(key))
            || button
                .child()
                .and_downcast::<adw::ButtonContent>()
                .is_some_and(|content| content.label() == text(key))
    })
    .expect("native action button")
    .emit_clicked();
}

fn entry(widget: &impl IsA<gtk::Widget>, title: &str) -> Option<adw::EntryRow> {
    if let Some(entry) = widget.as_ref().downcast_ref::<adw::EntryRow>() {
        if entry.title() == title {
            return Some(entry.clone());
        }
    }
    let mut child = widget.as_ref().first_child();
    while let Some(widget) = child {
        if let Some(entry) = entry(&widget, title) {
            return Some(entry);
        }
        child = widget.next_sibling();
    }
    None
}

fn capture(window: &adw::ApplicationWindow, name: &str) {
    let rendered = render_window(window);
    let Ok(directory) = std::env::var("VNIDROP_UI_SCREENSHOTS") else {
        return;
    };
    let path = std::path::Path::new(&directory);
    std::fs::create_dir_all(path).unwrap();
    rendered
        .save_to_png(path.join(format!("{name}.png")))
        .unwrap();
}

fn render_window(window: &adw::ApplicationWindow) -> gtk::gdk::Texture {
    let frames = Rc::new(Cell::new(0));
    let rendered = frames.clone();
    window.add_tick_callback(move |_, _| {
        rendered.set(rendered.get() + 1);
        if rendered.get() >= 5 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    window.queue_draw();
    until("screenshot frame", || frames.get() >= 5);
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let node = RefCell::new(None);
    until("rendered window", || {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            &snapshot,
            f64::from(window.width()),
            f64::from(window.height()),
        );
        node.replace(snapshot.to_node());
        if node.borrow().is_none() {
            window.queue_draw();
        }
        node.borrow().is_some()
    });
    let node = node.into_inner().unwrap();
    let renderer = gtk::gsk::CairoRenderer::new();
    renderer.realize(None::<&gtk::gdk::Surface>).unwrap();
    let texture = renderer.render_texture(&node, None);
    renderer.unrealize();
    texture
}

fn assert_scannable(window: &adw::ApplicationWindow, ticket: &str) {
    let texture = render_window(window);
    transfer_details_tests::assert_qr_background(window, &texture);
    let (width, height) = (texture.width() as usize, texture.height() as usize);
    let mut rgba = vec![0; width * height * 4];
    texture.download(&mut rgba, width * 4);
    let gray: Vec<_> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[0])
        .collect();
    let mut decoder = quircs::Quirc::default();
    assert!(
        decoder
            .identify(width, height, &gray)
            .filter_map(|code| code.ok())
            .filter_map(|code| code.decode().ok())
            .any(|code| code.payload == ticket.as_bytes()),
        "rendered QR must decode to the unchanged invitation"
    );
}

#[test]
fn native_draft_approval_receive_and_shutdown() {
    adw::init().expect("GTK display (use xvfb-run for headless testing)");
    gio::resources_register_include!("icons.gresource").expect("compiled icon resources");
    gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap())
        .add_resource_path("/com/vnidrop/VniDrop/icons");
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
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
    application.activate_action("send", None);
    until("send draft", || app.window.visible_dialog().is_some());
    let draft = app.window.visible_dialog().unwrap();
    let files = button(&draft, &|button| {
        button
            .child()
            .and_downcast::<adw::ButtonContent>()
            .is_some_and(|content| content.icon_name() == "document-open-symbolic")
    })
    .unwrap();
    let file_content = files.child().and_downcast::<adw::ButtonContent>().unwrap();
    assert_eq!(file_content.label(), text("button_choose_files"));
    assert!(button(&draft, &|button| {
        button
            .child()
            .and_downcast::<adw::ButtonContent>()
            .is_some_and(|content| content.icon_name() == "folder-open-symbolic")
    })
    .is_some());
    let submit = button(&draft, &|button| {
        button.label().as_deref() == Some(&text("linux_create_invitation"))
    })
    .unwrap();
    assert!(!submit.is_sensitive(), "empty draft cannot be submitted");
    capture(&app.window, "draft-empty");
    let composer = app.composer.borrow().clone().unwrap();
    composer.load_files(
        vec![gio::File::for_path(&source)],
        vnidrop_gnome::composer::SelectionMode::Replace,
    );
    until("selected source ready", || submit.is_sensitive());
    assert_eq!(file_content.icon_name(), "document-open-symbolic");
    assert_eq!(file_content.label(), text("button_change_files"));
    let name = entry(&draft, &text("field_transfer_name")).unwrap();
    let sender = entry(&draft, &text("field_sender_name")).unwrap();
    assert_eq!(name.text(), "résumé.txt");
    name.set_text("Research notes");
    sender.set_text("Native Sender");
    let extra = root.path().join("extra.txt");
    std::fs::write(&extra, b"extra").unwrap();
    composer.load_files(
        vec![gio::File::for_path(&extra)],
        vnidrop_gnome::composer::SelectionMode::Add,
    );
    until("additional source ready", || submit.is_sensitive());
    assert_eq!(name.text(), "Research notes");
    button(&draft, &|button| {
        button.tooltip_text().as_deref() == Some(&text("button_remove_file"))
    })
    .unwrap()
    .emit_clicked();
    assert_eq!(name.text(), "Research notes");
    button(&draft, &|button| {
        button.label().as_deref() == Some(&text("button_clear"))
    })
    .unwrap()
    .emit_clicked();
    assert!(!submit.is_sensitive());
    assert_eq!(name.text(), "");
    composer.load_files(
        vec![gio::File::for_path(&source)],
        vnidrop_gnome::composer::SelectionMode::Replace,
    );
    until("replacement source ready", || submit.is_sensitive());
    name.set_text("Research notes");
    let invalid = root.path().join("invalid.vnd");
    std::fs::write(&invalid, b"not an invitation").unwrap();
    app.enqueue_invitation(gio::File::for_path(&invalid));
    assert_eq!(
        app.pending.borrow().len(),
        1,
        "invitation review must wait for the draft"
    );
    assert_eq!(app.window.visible_dialog().as_ref(), Some(&draft));
    capture(&app.window, "draft-wide");
    app.window.set_default_size(390, 700);
    until("narrow draft", || app.window.width() < 500);
    capture(&app.window, "draft-narrow");
    application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceDark);
    capture(&app.window, "draft-narrow-dark");
    application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceLight);
    app.window.set_default_size(1080, 700);
    until("wide draft", || app.window.width() > 900);
    // The source can disappear after picking. A failed preparation must retain the form.
    std::fs::remove_file(&source).unwrap();
    submit.emit_clicked();
    until("preparation failure preserves editable draft", || {
        submit.is_sensitive()
    });
    assert_eq!(name.text(), "Research notes");
    assert_eq!(sender.text(), "Native Sender");
    assert_eq!(app.window.visible_dialog().as_ref(), Some(&draft));
    std::fs::write(&source, b"GTK approval verified bytes").unwrap();
    capture(&app.window, "draft-preparation-error");
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
    until("queued invitation processed after draft closes", || {
        app.pending.borrow().is_empty() && !app.reviewing.get()
    });
    let transfer = app
        .snapshot
        .borrow()
        .as_ref()
        .unwrap()
        .transfers
        .iter()
        .find(|transfer| transfer.status == "sharing")
        .unwrap()
        .clone();
    assert_eq!(transfer.transfer_name.as_deref(), Some("Research notes"));
    let inspection = app
        .session
        .borrow()
        .as_ref()
        .unwrap()
        .call(|core| core.inspect_ticket(transfer.ticket.clone().unwrap()))
        .unwrap();
    assert_eq!(
        inspection.metadata.sender_name.as_deref(),
        Some("Native Sender")
    );
    assert_eq!(
        app.selected.borrow().as_ref(),
        Some(&(transfer.transfer_id, "send".into()))
    );

    app.show_qr(transfer.transfer_id);
    until("QR ready", || {
        app.qr.borrow().as_ref().is_some_and(|qr| qr.ready.get())
    });
    assert_scannable(&app.window, transfer.ticket.as_ref().unwrap());
    capture(&app.window, "invitation-qr-wide");
    app.window.set_default_size(390, 700);
    until("narrow QR", || app.window.width() < 500);
    application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceDark);
    assert_scannable(&app.window, transfer.ticket.as_ref().unwrap());
    capture(&app.window, "invitation-qr-narrow-dark");
    app.window.visible_dialog().unwrap().close();
    until("QR closed", || app.qr.borrow().is_none());
    application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceLight);
    app.window.set_default_size(1080, 700);
    let qr = vnidrop_gnome::qr::InvitationQr::encode(transfer.ticket.as_ref().unwrap()).unwrap();
    let side = qr.width * 4;
    let pixels: Vec<_> = (0..side * side)
        .map(|i| qr.pixels[(i / side / 4) * qr.width + i % side / 4])
        .collect();
    let mut decoder = quircs::Quirc::default();
    let decoded = decoder
        .identify(side, side, &pixels)
        .next()
        .unwrap()
        .unwrap()
        .decode()
        .unwrap()
        .payload;
    assert_eq!(decoded, transfer.ticket.as_ref().unwrap().as_bytes());
    let scanned_ticket = String::from_utf8(decoded).unwrap();

    let receiver_profile = root.path().join("receiver");
    let destination = output.clone();
    let (done, completion) = mpsc::channel();
    let (shutdown_receiver, receiver_lifetime) = mpsc::channel::<bool>();
    let (receiver_ready, receiver_shared) = mpsc::channel();
    thread::spawn(move || {
        let receiver = Session::open(
            receiver_profile.to_str().unwrap().into(),
            vnidrop::CoreNetworkConfig {
                mode: vnidrop::CoreRelayMode::LocalOnly,
                relay_urls: vec![],
            },
        )
        .unwrap();
        receiver_ready.send(receiver.clone()).unwrap();
        let result = receiver.call(|core| {
            core.receive(
                scanned_ticket,
                destination.to_str().unwrap().into(),
                Some("Receiver".into()),
            )
        });
        done.send(result).unwrap();
        while let Ok(accept) = receiver_lifetime.recv_timeout(Duration::from_secs(60)) {
            if !accept {
                break;
            }
            let pending = receiver
                .call(|core| core.list_device_relationships())
                .unwrap()
                .into_iter()
                .find(|r| r.state == vnidrop::DeviceRelationshipState::PendingIncoming)
                .unwrap();
            receiver
                .call(|core| core.respond_to_device_pairing(pending.remote_endpoint_id, true))
                .unwrap();
        }
        receiver.close();
    });
    app.selected.replace(None);
    app.render_details();
    until("global approval banner", || {
        app.object::<gtk::Revealer>("requests").reveals_child()
    });
    app.show_devices();
    application.activate_action("review-requests", None);
    assert_eq!(
        app.selected.borrow().as_ref(),
        Some(&(transfer.transfer_id, "send".into()))
    );
    let approve = button(&app.details, &|button| {
        button.tooltip_text().as_deref() == Some(&text("button_approve"))
    })
    .unwrap();
    render_window(&app.window);
    let scroll = approve
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap();
    let bounds = approve.compute_bounds(&scroll).unwrap();
    assert!(
        bounds.y() >= 0.0 && bounds.y() + bounds.height() <= scroll.height() as f32,
        "Review must reveal the pending approval controls"
    );
    assert!(
        approve.has_focus(),
        "Review must focus the pending approval action"
    );
    let mut container = approve.parent();
    let mut highlighted = false;
    while let Some(widget) = container {
        highlighted |= widget.has_css_class("review-highlight");
        container = widget.parent();
    }
    assert!(highlighted, "Review must ring the receiver container");
    approve.emit_clicked();
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
        !app.object::<gtk::Revealer>("requests").reveals_child()
    });
    let receipt_started = Instant::now();
    let receipt_reported = Cell::new(false);
    until("sender delivery receipt", || {
        let snapshot = app.snapshot.borrow();
        let snapshot = snapshot.as_ref().unwrap();
        if receipt_started.elapsed() > Duration::from_secs(2) && !receipt_reported.replace(true) {
            eprintln!(
                "receipt: refreshing={}, busy={}, requests={:?}, events={:?}",
                app.refreshing.get(),
                app.busy.get(),
                snapshot
                    .requests
                    .iter()
                    .map(|r| &r.status)
                    .collect::<Vec<_>>(),
                snapshot
                    .events
                    .iter()
                    .map(|e| (&e.phase, &e.kind))
                    .collect::<Vec<_>>()
            );
        }
        !app.refreshing.get()
            && app.busy.get() == 0
            && snapshot.requests.iter().any(|request| {
                request.transfer_id == transfer.transfer_id && request.status == "completed"
            })
            && snapshot.events.iter().any(|event| {
                event.transfer_id == Some(transfer.transfer_id)
                    && event.kind == "receiver-completed"
            })
    });
    until("pairing eligibility", || {
        app.snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .devices
            .list(glib::real_time() / 1000)
            .iter()
            .any(|d| d.needs_attention())
    });
    app.review_device_request();
    let device_view = app.devices.borrow().clone().unwrap();
    let eligible = device_view.editor.borrow().as_ref().unwrap().target.clone();
    render_window(&app.window);
    assert_eq!(device_view.selected.borrow().as_ref(), Some(&eligible.peer));
    assert!(app.window.visible_dialog().is_none());
    assert!(device_view
        .editor
        .borrow()
        .as_ref()
        .unwrap()
        .primary
        .borrow()
        .as_ref()
        .unwrap()
        .has_focus());
    capture(&app.window, "device-remember");
    app.show_transfers();
    assert!(app
        .snapshot
        .borrow()
        .as_ref()
        .unwrap()
        .devices
        .saved
        .is_empty());
    app.review_device_request();
    activate_button(&device_view.details, "pairing_accept");
    until("outgoing pairing", || {
        app.snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .devices
            .relationships
            .iter()
            .any(|r| r.state == vnidrop::DeviceRelationshipState::PendingOutgoing)
    });
    capture(&app.window, "devices-pending");
    shutdown_receiver.send(true).unwrap();
    until("mutually saved device", || {
        !app.snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .devices
            .saved
            .is_empty()
    });
    let saved_device = app
        .snapshot
        .borrow()
        .as_ref()
        .unwrap()
        .devices
        .list(glib::real_time() / 1000)
        .into_iter()
        .find(|d| matches!(d.state, vnidrop_gnome::devices::DeviceState::Saved { .. }))
        .unwrap();
    device_view.show_device(&app, saved_device.clone());
    device_view.show_management(&app);
    render_window(&app.window);
    let device_details = app
        .window
        .visible_dialog()
        .expect("device management dialog");
    let label = entry(&device_details, &text("saved_devices_label_title")).unwrap();
    label.set_text("  Research laptop  ");
    app.refresh();
    until("device refresh keeps label draft", || !app.refreshing.get());
    assert_eq!(label.text(), "  Research laptop  ");
    device_details.close();
    until("management closed", || {
        app.window.visible_dialog().is_none()
    });
    app.show_transfers();
    app.show_devices();
    device_view.show_management(&app);
    render_window(&app.window);
    let device_details = app.window.visible_dialog().unwrap();
    assert_eq!(
        entry(&device_details, &text("saved_devices_label_title"))
            .unwrap()
            .text(),
        "  Research laptop  "
    );
    capture(&app.window, "device-management");
    activate_button(&device_details, "saved_devices_label_save");
    until("saved device label", || {
        app.snapshot.borrow().as_ref().unwrap().devices.saved[0]
            .local_label
            .as_deref()
            == Some("Research laptop")
    });
    until("management saved and closed", || {
        app.window.visible_dialog().is_none() && device_details.parent().is_none()
    });
    app.window.set_default_size(390, 700);
    until("narrow devices", || app.window.width() < 500);
    capture(&app.window, "devices-saved-narrow");
    let labeled = app
        .snapshot
        .borrow()
        .as_ref()
        .unwrap()
        .devices
        .list(glib::real_time() / 1000)
        .into_iter()
        .find(|d| d.peer == saved_device.peer)
        .unwrap();
    device_view.show_device(&app, labeled);
    device_view.show_management(&app);
    render_window(&app.window);
    let device_details = app.window.visible_dialog().unwrap();
    activate_button(&device_details, "saved_devices_block_action");
    until("block confirmation", || {
        app.window
            .visible_dialog()
            .is_some_and(|d| d.is::<adw::AlertDialog>())
    });
    capture(&app.window, "device-block-confirmation");
    render_window(&app.window);
    activate_button(&app.window.visible_dialog().unwrap(), "button_cancel");
    until("block cancelled", || app.window.visible_dialog().is_none());
    assert!(app
        .snapshot
        .borrow()
        .as_ref()
        .unwrap()
        .devices
        .blocked
        .is_empty());
    device_view.show_management(&app);
    render_window(&app.window);
    let device_details = app.window.visible_dialog().unwrap();
    activate_button(&device_details, "saved_devices_label_clear");
    until("cleared label", || {
        app.snapshot.borrow().as_ref().unwrap().devices.saved[0]
            .local_label
            .is_none()
    });
    until("cleared label closes management", || {
        app.window.visible_dialog().is_none()
    });
    app.show_transfers();
    assert!(!app.showing_devices.get());
    app.window.set_default_size(1080, 700);
    device_transfers_tests::native_saved_device_delivery(
        &app,
        receiver_shared
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        root.path(),
    );
    shutdown_receiver.send(false).unwrap();

    let saved = app.snapshot.borrow().clone().unwrap();
    let mut fixture = saved.clone();
    let request = fixture
        .requests
        .iter_mut()
        .find(|request| request.transfer_id == transfer.transfer_id)
        .unwrap();
    request.status = "accepted".into();
    request.completed_at = None;
    let request = request.clone();
    let event = |revision, kind: &str, bytes| {
        vnidrop::CoreEvent {
        id: format!("ui-progress-{revision}"), revision,
        timestamp: request.requested_at + revision as i64,
        scope: "transfer".into(), transfer_id: Some(transfer.transfer_id),
        direction: Some("send".into()), phase: "transfer".into(), kind: kind.into(),
        data_json: serde_json::json!({"connection_id": 1, "request_id": 1, "endpoint_id": request.remote_endpoint_id, "size": transfer.total_size, "end_offset": bytes}).to_string(),
    }
    };
    fixture.events.retain(|event| event.phase != "transfer");
    fixture.events.insert(0, event(1, "started", 0));
    fixture.events.insert(0, event(2, "progress", 13));
    // Render recorded progress in a window without live callbacks racing the fixture.
    let preview = App::new(&application, root.path().join("progress-view"));
    preview.set_ready(true);
    preview
        .selected
        .replace(Some((transfer.transfer_id, "send".into())));
    preview.split.set_show_content(true);
    preview.window.present();
    preview.snapshot.replace(Some(fixture.clone()));
    preview.render();
    let progress_root = preview.receiver_progress.borrow()[0].1.root.clone();
    let bar = std::iter::successors(progress_root.first_child(), |widget| widget.next_sibling())
        .find_map(|widget| widget.downcast::<gtk::ProgressBar>().ok())
        .unwrap();
    assert_eq!(bar.fraction(), 13.0 / transfer.total_size as f64);
    fixture.events.insert(0, event(3, "progress", 20));
    preview.snapshot.replace(Some(fixture));
    preview.render();
    assert_eq!(
        preview.receiver_progress.borrow()[0].1.root,
        progress_root,
        "byte updates preserve detail controls"
    );
    assert_eq!(bar.fraction(), 20.0 / transfer.total_size as f64);
    let scroll = preview
        .details
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    scroll.vadjustment().set_value(scroll.vadjustment().upper());
    capture(&preview.window, "transfer-progress-wide");
    preview.window.set_default_size(390, 700);
    until("narrow progress", || preview.window.width() < 500);
    scroll.vadjustment().set_value(scroll.vadjustment().upper());
    capture(&preview.window, "transfer-progress-narrow");
    preview.snapshot.replace(Some(saved));
    preview.render();
    preview.show_activity(transfer.transfer_id, "send".into());
    capture(&preview.window, "transfer-activity-narrow");
    preview.window.visible_dialog().unwrap().close();
    until("activity closed", || preview.activity.borrow().is_none());
    transfer_details_tests::histories_remain_bounded(&preview);

    preview.show_qr(transfer.transfer_id);
    until("preview QR ready", || {
        preview
            .qr
            .borrow()
            .as_ref()
            .is_some_and(|qr| qr.ready.get())
    });
    preview
        .snapshot
        .borrow_mut()
        .as_mut()
        .unwrap()
        .transfers
        .iter_mut()
        .find(|t| t.transfer_id == transfer.transfer_id)
        .unwrap()
        .status = "stopped".into();
    preview.render();
    until("stopped share dismisses QR", || {
        preview.qr.borrow().is_none()
    });
    preview.show_qr(transfer.transfer_id);
    assert!(preview.qr.borrow().is_none());
    {
        let mut snapshot = preview.snapshot.borrow_mut();
        let transfer = snapshot
            .as_mut()
            .unwrap()
            .transfers
            .iter_mut()
            .find(|t| t.transfer_id == transfer.transfer_id)
            .unwrap();
        transfer.status = "sharing".into();
        transfer.ticket = Some("a".repeat(2954));
    }
    preview.show_qr(transfer.transfer_id);
    until("oversized QR fallback", || {
        preview
            .qr
            .borrow()
            .as_ref()
            .is_some_and(|qr| qr.ready.get())
    });
    capture(&preview.window, "invitation-qr-unavailable");
    preview.window.visible_dialog().unwrap().close();
    until("fallback closed", || preview.qr.borrow().is_none());
    navigation_tests::review_routes_to_the_pending_decision(&preview);
    navigation_tests::notification_routes_after_modal_closes(&preview);
    preview.window.destroy();

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
    settings_ui_tests::settings_restart_and_preview_workflow(root.path());
    reporting_ui_tests::report_failure_retry_and_receipt(root.path());
}
