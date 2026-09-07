use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::glib;
use vnidrop::ReceiverRequest;

use super::{dialogs::scrollable_content, i18n::text, App};

pub(super) struct ReceiverHistory {
    rows: gtk::ListBox,
    transfer_id: u64,
    rendered: RefCell<String>,
}

pub(super) fn is_finished(status: &str) -> bool {
    matches!(
        status,
        "completed" | "refused" | "expired" | "failed" | "cancelled" | "aborted"
    )
}

pub(super) fn status_key(status: &str) -> &'static str {
    match status {
        "requested" => "transfer_receiver_requested",
        "accepted" => "transfer_receiver_accepted",
        "completed" => "transfer_receiver_completed",
        "refused" => "transfer_receiver_refused",
        "expired" => "transfer_receiver_expired",
        "failed" => "transfer_receiver_failed",
        "cancelled" | "aborted" => "status_cancelled",
        _ => "transfer_receiver_unknown",
    }
}

impl App {
    pub(super) fn show_receiver_history(self: &Rc<Self>, transfer_id: u64) {
        if self.window.visible_dialog().is_some() {
            return;
        }
        let (dialog, content) = scrollable_content("linux_receiver_history");
        let rows = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        rows.add_css_class("boxed-list");
        content.append(&rows);
        let history = Rc::new(ReceiverHistory {
            rows,
            transfer_id,
            rendered: RefCell::new(String::new()),
        });
        if let Some(snapshot) = self.snapshot.borrow().as_ref() {
            history.render(&snapshot.requests);
        }
        let weak = Rc::downgrade(self);
        dialog.connect_closed(move |_| {
            if let Some(app) = weak.upgrade() {
                app.receiver_history.borrow_mut().take();
            }
        });
        self.receiver_history.replace(Some(history));
        dialog.present(Some(&self.window));
    }
}

impl ReceiverHistory {
    pub fn render(&self, requests: &[ReceiverRequest]) {
        let mut requests: Vec<_> = requests
            .iter()
            .filter(|request| {
                request.transfer_id == self.transfer_id && is_finished(&request.status)
            })
            .collect();
        requests.sort_by(|a, b| {
            b.completed_at
                .or(b.responded_at)
                .unwrap_or(b.requested_at)
                .cmp(&a.completed_at.or(a.responded_at).unwrap_or(a.requested_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        let fingerprint = serde_json::to_string(&requests).unwrap();
        if *self.rendered.borrow() == fingerprint {
            return;
        }
        self.rendered.replace(fingerprint);
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        for request in requests {
            let name = request
                .receiver_name
                .as_deref()
                .or(request.receiver_device_name.as_deref())
                .map(str::to_owned)
                .unwrap_or_else(|| text("approval_nearby_device"));
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&name))
                .subtitle(text(status_key(&request.status)))
                .title_lines(2)
                .subtitle_lines(2)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name(match request.status.as_str() {
                "completed" => "emblem-ok-symbolic",
                "failed" => "dialog-error-symbolic",
                _ => "process-stop-symbolic",
            }));
            self.rows.append(&row);
        }
    }
}
