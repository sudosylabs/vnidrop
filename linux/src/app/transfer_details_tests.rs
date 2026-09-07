use super::*;

fn descendant<T: IsA<gtk::Widget> + glib::object::IsClass>(
    widget: &impl IsA<gtk::Widget>,
) -> Option<T> {
    if let Ok(found) = widget.as_ref().clone().downcast::<T>() {
        return Some(found);
    }
    let mut child = widget.as_ref().first_child();
    while let Some(widget) = child {
        if let Some(found) = descendant::<T>(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

pub(super) fn assert_qr_background(window: &adw::ApplicationWindow, texture: &gtk::gdk::Texture) {
    if !window
        .application()
        .and_downcast::<adw::Application>()
        .unwrap()
        .style_manager()
        .is_dark()
    {
        return;
    }
    let drawing = descendant::<gtk::DrawingArea>(&window.visible_dialog().unwrap()).unwrap();
    let bounds = drawing.compute_bounds(window).unwrap();
    let stride = texture.width() as usize * 4;
    let mut pixels = vec![0; stride * texture.height() as usize];
    texture.download(&mut pixels, stride);
    let offset = (bounds.y() as usize + 2) * stride + (bounds.x() as usize + 2) * 4;
    assert!(
        pixels[offset..offset + 3]
            .iter()
            .any(|channel| *channel < 250),
        "QR white background must be confined to the square code, not the drawing area"
    );
}

fn row_count(list: &gtk::ListBox) -> usize {
    let mut count = 0;
    let mut child = list.first_child();
    while let Some(row) = child {
        count += 1;
        child = row.next_sibling();
    }
    count
}

fn assert_scrollable(app: &Rc<App>, dialog: &adw::Dialog, expected_rows: usize) {
    render_window(&app.window);
    let scroll = descendant::<gtk::ScrolledWindow>(dialog).unwrap();
    let list = descendant::<gtk::ListBox>(dialog).unwrap();
    assert_eq!(row_count(&list), expected_rows);
    let adjustment = scroll.vadjustment();
    assert!(
        adjustment.upper() > adjustment.page_size(),
        "Long history must scroll within the dialog"
    );
    adjustment.set_value(adjustment.upper());
    render_window(&app.window);
    let last = list.last_child().unwrap().compute_bounds(&scroll).unwrap();
    assert!(
        last.y() >= 0.0 && last.y() + last.height() <= scroll.height() as f32,
        "The final history row must be reachable by scrolling"
    );
}

pub(super) fn histories_remain_bounded(app: &Rc<App>) {
    let saved = app.snapshot.borrow().clone().unwrap();
    let template = saved.requests[0].clone();
    let id = template.transfer_id;
    app.window.set_default_size(1080, 700);
    until("wide history dialogs", || app.window.width() > 900);
    let mut fixture = saved.clone();
    fixture.requests = (0..40)
        .map(|i| {
            let mut request = template.clone();
            request.id = format!("finished-{i}");
            request.receiver_name = Some(format!("Receiver {i}"));
            request.status = [
                "completed",
                "failed",
                "refused",
                "expired",
                "cancelled",
                "aborted",
            ][i % 6]
                .into();
            request
        })
        .collect();
    for status in ["requested", "accepted", "future-status"] {
        let mut request = template.clone();
        request.id = status.into();
        request.status = status.into();
        fixture.requests.push(request);
    }
    let mut unrelated = template.clone();
    unrelated.id = "other-transfer".into();
    unrelated.transfer_id += 1;
    fixture.requests.push(unrelated);
    app.snapshot.replace(Some(fixture));
    app.render();
    assert_eq!(
        app.receiver_progress.borrow().len(),
        3,
        "Only ongoing or unknown receiver states stay on the page"
    );
    assert_eq!(
        app.review_actions.borrow().len(),
        1,
        "Pending approvals remain reachable"
    );
    render_window(&app.window);
    let mut card = app
        .review_actions
        .borrow()
        .values()
        .next()
        .unwrap()
        .clone()
        .upcast::<gtk::Widget>();
    while !card.has_css_class("review-target") {
        card = card.parent().unwrap();
    }
    fn history_row(widget: &gtk::Widget) -> Option<adw::ActionRow> {
        if let Some(row) = widget.downcast_ref::<adw::ActionRow>() {
            if row.title() == text("linux_receiver_history") {
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
    let history_row = history_row(app.details.upcast_ref()).unwrap();
    let history_bounds = history_row.compute_bounds(&app.details).unwrap();
    let card_bounds = card.compute_bounds(&app.details).unwrap();
    assert!(
        card_bounds.y() - history_bounds.y() - history_bounds.height() >= 12.0,
        "Receiver history needs spacing before the active receiver cards"
    );
    app.show_receiver_history(id);
    let history = app.window.visible_dialog().unwrap();
    assert_scrollable(app, &history, 40);
    let height = history.height();
    app.snapshot
        .borrow_mut()
        .as_mut()
        .unwrap()
        .requests
        .iter_mut()
        .find(|request| request.id == "accepted")
        .unwrap()
        .status = "completed".into();
    app.render();
    assert_scrollable(app, &history, 41);
    assert_eq!(
        history.height(),
        height,
        "Receiver history must not grow with more rows"
    );
    capture(&app.window, "receiver-history-wide");
    app.window.set_default_size(390, 700);
    until("narrow receiver history", || app.window.width() < 500);
    assert_scrollable(app, &history, 41);
    capture(&app.window, "receiver-history-narrow");
    history.close();
    until("receiver history closed", || {
        app.receiver_history.borrow().is_none()
    });
    app.snapshot.replace(Some(saved.clone()));
    app.render();
    app.window.set_default_size(1080, 700);
    until("wide activity history", || app.window.width() > 900);
    app.show_activity(id, "send".into());
    render_window(&app.window);
    let activity = app.window.visible_dialog().unwrap();
    let height = activity.height();
    let mut event = saved.events[0].clone();
    event.transfer_id = Some(id);
    event.direction = Some("send".into());
    event.kind = "receiver-completed".into();
    app.snapshot.borrow_mut().as_mut().unwrap().events = (0..80)
        .map(|i| {
            let mut event = event.clone();
            event.id = format!("history-{i}");
            event
        })
        .collect();
    app.render();
    assert_scrollable(app, &activity, 80);
    assert_eq!(
        activity.height(),
        height,
        "Activity must keep its height when events arrive"
    );
    capture(&app.window, "activity-long-wide");
    app.window.set_default_size(390, 700);
    until("narrow activity history", || app.window.width() < 500);
    assert_scrollable(app, &activity, 80);
    capture(&app.window, "activity-long-narrow");
    activity.close();
    until("long activity closed", || app.activity.borrow().is_none());
    app.snapshot.replace(Some(saved));
    app.render();
}
