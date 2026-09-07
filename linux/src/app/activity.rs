use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::glib;
use vnidrop::CoreEvent;
use vnidrop_gnome::presentation::activity_key;

use super::{dialogs::scrollable_content, i18n::text, App};

pub(super) struct Activity {
    dialog: adw::Dialog,
    rows: gtk::ListBox,
    empty: gtk::Label,
    transfer_id: u64,
    direction: String,
    rendered: RefCell<Vec<String>>,
}

impl App {
    pub(super) fn show_activity(self: &Rc<Self>, transfer_id: u64, direction: String) {
        if self.window.visible_dialog().is_some() {
            return;
        }
        let (dialog, content) = scrollable_content("transfer_activity_title");
        let rows = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        rows.add_css_class("boxed-list");
        let empty = gtk::Label::builder()
            .label(text("transfer_no_activity"))
            .wrap(true)
            .build();
        content.append(&empty);
        content.append(&rows);
        let activity = Rc::new(Activity {
            dialog,
            rows,
            empty,
            transfer_id,
            direction,
            rendered: RefCell::new(Vec::new()),
        });
        if let Some(snapshot) = self.snapshot.borrow().as_ref() {
            activity.render(&snapshot.events);
        }
        let weak = Rc::downgrade(self);
        activity.dialog.connect_closed(move |_| {
            if let Some(app) = weak.upgrade() {
                app.activity.borrow_mut().take();
            }
        });
        self.activity.replace(Some(activity.clone()));
        activity.dialog.present(Some(&self.window));
    }
}

impl Activity {
    pub fn render(&self, events: &[CoreEvent]) {
        let events: Vec<_> = events
            .iter()
            .filter(|event| {
                event.transfer_id == Some(self.transfer_id)
                    && event.direction.as_deref() == Some(self.direction.as_str())
            })
            .filter_map(|event| activity_key(event).map(|key| (event, key)))
            .collect();
        let ids: Vec<_> = events.iter().map(|(event, _)| event.id.clone()).collect();
        self.empty.set_visible(events.is_empty());
        self.rows.set_visible(!events.is_empty());
        if *self.rendered.borrow() == ids {
            return;
        }
        self.rendered.replace(ids);
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        for (event, key) in events {
            let time = glib::DateTime::from_unix_local(event.timestamp.div_euclid(1000))
                .and_then(|date| date.format("%x %X"))
                .map(|time| time.to_string())
                .unwrap_or_else(|_| text("value_unavailable"));
            let row = adw::ActionRow::builder()
                .title(text(key))
                .subtitle(time)
                .title_lines(2)
                .build();
            let icon = match key {
                "transfer_event_failed" => "dialog-error-symbolic",
                "transfer_event_completed" | "progress_completed" => "emblem-ok-symbolic",
                "transfer_event_requested" => "dialog-question-symbolic",
                "transfer_event_stopped" | "transfer_event_refused" => "process-stop-symbolic",
                "transfer_event_ready" => "document-send-symbolic",
                _ => "document-open-recent-symbolic",
            };
            row.add_prefix(&gtk::Image::from_icon_name(icon));
            self.rows.append(&row);
        }
    }
}
