use super::*;

fn assert_review_visible(app: &Rc<App>, button: &gtk::Button) {
    render_window(&app.window);
    let scroll = button.ancestor(gtk::ScrolledWindow::static_type()).unwrap();
    let bounds = button.compute_bounds(&scroll).unwrap();
    assert!(
        bounds.y() >= 0.0 && bounds.y() + bounds.height() <= scroll.height() as f32,
        "Review must bring the decision into the visible scroll area"
    );
    assert!(button.has_focus(), "Review must focus the decision");
}

pub(super) fn review_routes_to_the_pending_decision(app: &Rc<App>) {
    let mut snapshot = app.snapshot.borrow().clone().unwrap();
    let mut oldest = snapshot.requests[0].clone();
    oldest.id = "oldest-pending".into();
    oldest.status = "requested".into();
    oldest.requested_at = 1;
    let mut newer = oldest.clone();
    newer.id = "newer-pending".into();
    newer.requested_at = 2;
    snapshot.requests = vec![newer, oldest.clone()];
    app.snapshot.replace(Some(snapshot));
    app.show_devices();
    app.window.set_default_size(390, 700);
    until("narrow review route", || app.window.width() < 500);
    app.review_transfer_request();
    assert!(!app.showing_devices.get());
    assert_eq!(
        app.selected.borrow().as_ref(),
        Some(&(oldest.transfer_id, "send".into()))
    );
    let approve = app.review_actions.borrow().get(&oldest.id).unwrap().clone();
    assert_review_visible(app, &approve);
    let mut card = approve.parent().unwrap();
    while !card.has_css_class("review-target") {
        card = card.parent().unwrap();
    }
    assert!(
        card.has_css_class("review-highlight"),
        "Review must ring the receiver container"
    );
    let next = card.prev_sibling().expect("second receiver card");
    let bounds = card.compute_bounds(&next.parent().unwrap()).unwrap();
    let next_bounds = next.compute_bounds(&next.parent().unwrap()).unwrap();
    assert!(
        (next_bounds.y() - bounds.y()).abs() >= bounds.height() + 8.0,
        "Receiver cards must have visible separation"
    );
    let scroll = card.ancestor(gtk::ScrolledWindow::static_type()).unwrap();
    let bounds = card.compute_bounds(&scroll).unwrap();
    assert!(
        bounds.y() >= 0.0 && bounds.y() + bounds.height() <= scroll.height() as f32,
        "Review must reveal the whole receiver container, including its ring"
    );
    capture(&app.window, "review-transfer-narrow");
    app.application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceDark);
    capture(&app.window, "review-transfer-narrow-dark");
    app.application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceLight);
    app.render();
    assert_review_visible(app, &approve);

    let peer = "pending-device-fixture";
    {
        let mut snapshot = app.snapshot.borrow_mut();
        let devices = &mut snapshot.as_mut().unwrap().devices;
        devices.relationships.push(vnidrop::DeviceRelationship {
            remote_endpoint_id: peer.into(),
            state: vnidrop::DeviceRelationshipState::PendingIncoming,
            generation: 1,
            minimum_protocol_version: 2,
            created_at: 1,
            updated_at: 1,
        });
    }
    app.review_device_request();
    let view = app.devices.borrow().clone().unwrap();
    assert!(view.split.is_collapsed());
    assert_eq!(view.selected.borrow().as_deref(), Some(peer));
    let allow = view
        .editor
        .borrow()
        .as_ref()
        .unwrap()
        .primary
        .borrow()
        .clone()
        .unwrap();
    assert_review_visible(app, &allow);
    assert!(app.window.visible_dialog().is_none());
    capture(&app.window, "review-device-narrow");
    view.split.set_show_content(false);
    render_window(&app.window);
    assert_eq!(view.selected.borrow().as_deref(), Some(peer));
    capture(&app.window, "devices-list-narrow");
    app.review_device_request();
    assert_review_visible(app, &allow);
    app.window.set_default_size(1080, 700);
    until("wide review route", || app.window.width() > 900);
    assert!(!view.split.is_collapsed());
    app.review_device_request();
    assert_review_visible(app, &allow);
    capture(&app.window, "devices-page-wide");
    let snapshot = app.snapshot.borrow().clone().unwrap();
    app.snapshot.borrow_mut().as_mut().unwrap().devices = Default::default();
    app.show_devices();
    render_window(&app.window);
    assert_eq!(view.root.visible_child_name().as_deref(), Some("empty"));
    let empty = view.root.visible_child().unwrap();
    assert_eq!(
        empty.width(),
        view.root.width(),
        "Empty state spans the Devices page"
    );
    capture(&app.window, "devices-empty-wide");
    app.application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceDark);
    capture(&app.window, "devices-empty-wide-dark");
    app.application
        .style_manager()
        .set_color_scheme(adw::ColorScheme::ForceLight);
    app.window.set_default_size(390, 700);
    until("narrow empty devices", || app.window.width() < 500);
    capture(&app.window, "devices-empty-narrow");
    app.snapshot.replace(Some(snapshot));
}
