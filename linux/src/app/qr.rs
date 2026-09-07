use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use vnidrop_gnome::{presentation, qr::InvitationQr, session::Snapshot};

use super::{dialogs::content, i18n::text, App};

pub(super) struct QrDialog {
    pub dialog: adw::Dialog,
    transfer_id: u64,
    ticket: String,
    pub ready: std::cell::Cell<bool>,
}

impl App {
    pub(super) fn show_qr(self: &Rc<Self>, transfer_id: u64) {
        if self.window.visible_dialog().is_some() {
            return;
        }
        let ticket = self.snapshot.borrow().as_ref().and_then(|snapshot| {
            snapshot
                .transfers
                .iter()
                .find(|t| t.transfer_id == transfer_id && t.direction == "send")
                .and_then(presentation::invitation)
                .map(str::to_owned)
        });
        let Some(ticket) = ticket else {
            return;
        };
        let (dialog, rows, _) = content("linux_show_qr");
        dialog.set_content_width(600);
        dialog.set_content_height(620);
        let spinner = gtk::Spinner::builder().spinning(true).build();
        rows.append(&spinner);
        let caption = gtk::Label::builder()
            .label(text("transfer_scan_qr"))
            .wrap(true)
            .build();
        rows.append(&caption);
        let state = Rc::new(QrDialog {
            dialog,
            transfer_id,
            ticket: ticket.clone(),
            ready: std::cell::Cell::new(false),
        });
        let weak = Rc::downgrade(self);
        state.dialog.connect_closed(move |_| {
            if let Some(app) = weak.upgrade() {
                app.qr.borrow_mut().take();
            }
        });
        self.qr.replace(Some(state.clone()));
        state.dialog.present(Some(&self.window));
        let weak = Rc::downgrade(&state);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || InvitationQr::encode(&ticket)).await;
            let Some(state) = weak.upgrade() else {
                return;
            };
            spinner.stop();
            rows.remove(&spinner);
            match result {
                Ok(Ok(code)) => {
                    let drawing = gtk::DrawingArea::builder()
                        .hexpand(true)
                        .content_height(400)
                        .build();
                    drawing.update_property(&[gtk::accessible::Property::Label(&text(
                        "transfer_scan_qr",
                    ))]);
                    drawing.set_draw_func(move |_, context, width, height| {
                        let scale = (f64::from(width.min(height)) / code.width as f64)
                            .floor()
                            .max(1.0);
                        let x0 = ((f64::from(width) - code.width as f64 * scale) / 2.0).floor();
                        let y0 = ((f64::from(height) - code.width as f64 * scale) / 2.0).floor();
                        context.set_antialias(gtk::cairo::Antialias::None);
                        context.set_source_rgb(1.0, 1.0, 1.0);
                        context.rectangle(
                            x0,
                            y0,
                            code.width as f64 * scale,
                            code.width as f64 * scale,
                        );
                        let _ = context.fill();
                        context.set_source_rgb(0.0, 0.0, 0.0);
                        for (index, pixel) in code.pixels.iter().enumerate() {
                            if *pixel == 0 {
                                context.rectangle(
                                    x0 + (index % code.width) as f64 * scale,
                                    y0 + (index / code.width) as f64 * scale,
                                    scale,
                                    scale,
                                );
                            }
                        }
                        let _ = context.fill();
                    });
                    rows.prepend(&drawing);
                }
                Ok(Err(_)) | Err(_) => caption.set_label(&text("linux_qr_unavailable")),
            }
            state.ready.set(true);
        });
    }
}

impl QrDialog {
    pub fn refresh(&self, snapshot: &Snapshot) {
        let current = snapshot
            .transfers
            .iter()
            .find(|t| t.transfer_id == self.transfer_id && t.direction == "send")
            .and_then(presentation::invitation);
        if current != Some(self.ticket.as_str()) {
            self.dialog.close();
        }
    }
}
