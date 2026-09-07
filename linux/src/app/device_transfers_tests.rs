use super::*;
use vnidrop::TargetedTransferState as State;

fn offer_files(app: &Rc<App>, peer: &vnidrop_gnome::devices::Device, source: &std::path::Path) {
    app.show_devices();
    let view = app.devices.borrow().clone().unwrap();
    view.show_device(app, peer.clone());
    activate_button(&view.details, "saved_devices_send_action");
    let dialog = app.window.visible_dialog().unwrap();
    assert!(
        entry(&dialog, &text("field_sender_name")).is_none(),
        "Direct sending must not expose invitation sender controls"
    );
    app.compose(vec![gio::File::for_path(source)]);
    until("direct draft files ready", || app.busy.get() == 0);
    render_window(&app.window);
    capture(&app.window, "device-send-draft");
    activate_button(&dialog, "saved_devices_send_action");
    until("direct offer registered", || {
        app.composer.borrow().is_none()
    });
}

pub(super) fn native_saved_device_delivery(
    sender: &Rc<App>,
    receiver: Arc<Session>,
    root: &std::path::Path,
) {
    let application = adw::Application::builder()
        .application_id("com.vnidrop.VniDrop.DirectUITest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let app = App::new(&application, root.join("receiver"));
    let output = root.join("native-direct-output");
    std::fs::create_dir(&output).unwrap();
    app.preferences.borrow_mut().receive_directory = output.clone();
    app.session.replace(Some(receiver));
    app.set_ready(true);
    app.window.present();
    app.refresh();
    until("direct receiver snapshot", || {
        app.snapshot.borrow().is_some()
    });
    let device = sender
        .snapshot
        .borrow()
        .as_ref()
        .unwrap()
        .devices
        .list(glib::real_time() / 1000)
        .into_iter()
        .find(|d| matches!(d.state, vnidrop_gnome::devices::DeviceState::Saved { .. }))
        .unwrap();
    let source = root.join("direct-ui.txt");
    std::fs::write(&source, b"native device UI delivery").unwrap();
    offer_files(sender, &device, &source);
    until("native incoming offer", || {
        app.refresh();
        !app.snapshot.borrow().as_ref().unwrap().offers.is_empty()
    });
    let offer = app.snapshot.borrow().as_ref().unwrap().offers[0].clone();
    app.review_device_request();
    let view = app.devices.borrow().clone().unwrap();
    let panel = view.transfers.borrow().clone().unwrap();
    let approve = panel
        .approvals
        .borrow()
        .get(&offer.transfer_id)
        .unwrap()
        .clone();
    render_window(&app.window);
    assert!(
        approve.has_focus(),
        "Review must focus the incoming direct offer"
    );
    capture(&app.window, "device-incoming-offer-wide");
    app.window.set_default_size(390, 700);
    until("narrow incoming offer", || app.window.width() < 500);
    capture(&app.window, "device-incoming-offer-narrow");
    approve.emit_clicked();
    until("native direct receive completed", || {
        app.refresh();
        app.snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .targeted
            .iter()
            .any(|t| t.id == offer.transfer_id && t.state == State::Completed)
    });
    until("native direct sender receipt", || {
        sender
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .targeted
            .iter()
            .any(|t| t.id == offer.transfer_id && t.state == State::Completed)
    });
    fn has_payload(path: &std::path::Path) -> bool {
        std::fs::read_dir(path).unwrap().any(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                has_payload(&path)
            } else {
                std::fs::read(path).unwrap() == b"native device UI delivery"
            }
        })
    }
    assert!(has_payload(&output));
    capture(&app.window, "device-received-history");
    offer_files(sender, &device, &source);
    until("native offer to cancel", || {
        app.refresh();
        !app.snapshot.borrow().as_ref().unwrap().offers.is_empty()
    });
    let cancelled = app.snapshot.borrow().as_ref().unwrap().offers[0]
        .transfer_id
        .clone();
    until("sender waiting for direct approval", || {
        sender
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .targeted
            .iter()
            .any(|t| t.id == cancelled && t.state == State::AwaitingApproval)
    });
    let sender_view = sender.devices.borrow().clone().unwrap();
    activate_button(&sender_view.details, "saved_devices_transfer_cancel");
    until("direct cancellation confirmation", || {
        sender.window.visible_dialog().is_some()
    });
    render_window(&sender.window);
    activate_button(
        &sender.window.visible_dialog().unwrap(),
        "linux_cancel_direct_transfer",
    );
    until("direct transfer cancelled", || {
        sender
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .targeted
            .iter()
            .any(|t| t.id == cancelled && t.state == State::Cancelled)
    });
    until("cancel confirmation closed", || {
        sender.window.visible_dialog().is_none()
    });
    until("cancelled offer removed", || {
        app.refresh();
        app.snapshot.borrow().as_ref().unwrap().offers.is_empty()
    });
    offer_files(sender, &device, &source);
    until("second native offer", || {
        app.refresh();
        !app.snapshot.borrow().as_ref().unwrap().offers.is_empty()
    });
    let declined = app.snapshot.borrow().as_ref().unwrap().offers[0]
        .transfer_id
        .clone();
    app.review_device_request();
    activate_button(&view.details, "button_refuse");
    until("native offer declined", || {
        sender
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .targeted
            .iter()
            .any(|t| t.id == declined && t.state == State::Declined)
    });
    offer_files(sender, &device, &source);
    until("decline cooldown prevents another prompt", || {
        sender
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .targeted
            .iter()
            .any(|t| t.state == State::Failed)
    });
    until("declined offer removed", || {
        app.refresh();
        app.snapshot.borrow().as_ref().unwrap().offers.is_empty()
    });
    fn history_row(widget: &gtk::Widget) -> Option<adw::ActionRow> {
        if let Some(row) = widget.downcast_ref::<adw::ActionRow>() {
            if row.title() == text("linux_device_transfer_history") {
                return Some(row.clone());
            }
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(row) = history_row(&widget) {
                return Some(row);
            }
            child = widget.next_sibling();
        }
        None
    }
    history_row(sender_view.details.upcast_ref())
        .unwrap()
        .emit_by_name::<()>("activated", &[]);
    until("native device transfer history", || {
        sender.targeted_history.borrow().is_some()
    });
    capture(&sender.window, "device-transfer-history-dialog");
    let history_before = sender.snapshot.borrow().as_ref().unwrap().targeted.clone();
    activate_button(
        &sender.window.visible_dialog().unwrap(),
        "saved_devices_transfer_delete",
    );
    until("direct delete confirmation", || {
        sender
            .window
            .visible_dialog()
            .is_some_and(|dialog| dialog.is::<adw::AlertDialog>())
    });
    render_window(&sender.window);
    activate_button(
        &sender.window.visible_dialog().unwrap(),
        "saved_devices_transfer_delete",
    );
    until("direct history deletion", || {
        let snapshot = sender.snapshot.borrow();
        let remaining = &snapshot.as_ref().unwrap().targeted;
        history_before
            .iter()
            .filter(|before| {
                !remaining
                    .iter()
                    .any(|after| after.id == before.id && after.state != State::Deleted)
            })
            .count()
            == 1
    });
    until("direct delete confirmation closed", || {
        sender.window.visible_dialog().is_none()
    });
    app.session.borrow_mut().take();
    app.closing.set(true);
    app.window.destroy();
    sender.show_devices();
    capture(&sender.window, "device-transfer-history");
    sender.show_transfers();
}
