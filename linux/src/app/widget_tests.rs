use super::*;

pub(super) fn readiness_and_clipboard() {
    let dispatches = Rc::new(Cell::new(0));
    let pending = dispatches.clone();
    let source = glib::idle_add_local(move || {
        pending.set(pending.get() + 1);
        if pending.get() < 1000 {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
    until("readiness while background events remain pending", || {
        dispatches.get() > 0
    });
    assert!(
        dispatches.get() < 1000,
        "Readiness must be observed before draining a continuously pending source"
    );
    source.remove();
    let label = gtk::Label::new(Some("Clipboard lifetime fixture"));
    label.set_selectable(true);
    super::super::widgets::protect_selection(&label);
    let clipboard = label.primary_clipboard();
    label.select_region(0, -1);
    assert!(clipboard.content().is_some());
    drop(label);
    clipboard.set_text("Replacement selection");
    fn selectable_label(widget: &gtk::Widget) -> Option<gtk::Label> {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if label.is_selectable() {
                return Some(label.clone());
            }
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(label) = selectable_label(&widget) {
                return Some(label);
            }
            child = widget.next_sibling();
        }
        None
    }
    let row = super::super::dialogs::fact("field_username", "Subtitle clipboard fixture");
    let label = selectable_label(row.upcast_ref()).unwrap();
    label.select_region(0, -1);
    assert!(clipboard.content().is_some());
    drop(label);
    drop(row);
    clipboard.set_text("Replacement subtitle selection");
}

pub(super) fn early_close(app: &Rc<App>) {
    let (early, _, _) = super::super::dialogs::content("linux_about");
    early.present(Some(&app.window));
    super::super::dialogs::close(&early);
    render_window(&app.window);
    assert!(
        app.window.visible_dialog().is_none(),
        "A completion before the first dialog frame must close the dialog"
    );
    let (guarded, _, _) = super::super::dialogs::content("linux_about");
    guarded.set_can_close(false);
    guarded.present(Some(&app.window));
    super::super::dialogs::close(&guarded);
    render_window(&app.window);
    assert_eq!(app.window.visible_dialog().as_ref(), Some(&guarded));
    guarded.set_can_close(true);
    super::super::dialogs::close(&guarded);
    render_window(&app.window);
    assert!(app.window.visible_dialog().is_none());
}

pub(super) fn assert_visible_choice(row: &adw::ComboRow) {
    let value = row
        .selected_item()
        .and_downcast::<gtk::StringObject>()
        .unwrap()
        .string();
    fn visible(widget: &gtk::Widget, value: &str) -> bool {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if label.is_mapped() && label.text() == value && label.width() > 0 {
                return true;
            }
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if visible(&widget, value) {
                return true;
            }
            child = widget.next_sibling();
        }
        false
    }
    assert!(
        visible(row.upcast_ref(), &value),
        "Selected value must be visible while closed: {value}"
    );
}

pub(super) fn composer_controls(app: &Rc<App>) {
    let dialog = app.window.visible_dialog().unwrap();
    let policy =
        super::settings_ui_tests::combo(dialog.upcast_ref(), &text("send_access_title")).unwrap();
    for selection in [0, 1, 0] {
        policy.set_selected(selection);
        render_window(&app.window);
        assert_visible_choice(&policy);
    }
    let settings = gtk::Settings::default().unwrap();
    let previous = settings.gtk_decoration_layout();
    for layout in ["close:minimize,maximize", ":minimize,maximize,close"] {
        settings.set_gtk_decoration_layout(Some(layout));
        render_window(&app.window);
        assert!(
            button(&dialog, &|button| button.has_css_class("close")
                && button.is_mapped())
            .is_none(),
            "Cancel must be the only visible draft dismissal button"
        );
    }
    capture(&app.window, "draft-dismissal-controls");
    settings.set_gtk_decoration_layout(previous.as_deref());
}

pub(super) fn stop_confirmation(app: &Rc<App>) {
    activate_button(&app.details, "send_stop_sharing");
    until("stop confirmation", || {
        app.window.visible_dialog().is_some()
    });
    let dialog = app
        .window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    capture(&app.window, "stop-confirmation");
    assert_ne!(
        dialog.response_label("cancel"),
        dialog.response_label("confirm"),
        "Dismissal and stopping must have distinct labels"
    );
    activate_button(&dialog, "button_cancel");
    until("stop dismissed", || app.window.visible_dialog().is_none());
    assert!(app
        .snapshot
        .borrow()
        .as_ref()
        .unwrap()
        .transfers
        .iter()
        .any(|transfer| transfer.status == "sharing"));
}
