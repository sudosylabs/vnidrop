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
