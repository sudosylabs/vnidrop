use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use vnidrop_gnome::{invitation, presentation};

use super::{
    dialogs::{self, fact},
    i18n::{format, text},
    App,
};

pub(super) struct TransferRow {
    key: (u64, String),
    row: adw::ActionRow,
    spinner: gtk::Spinner,
}

impl App {
    pub(super) fn render(self: &Rc<Self>) {
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            return;
        };
        let pending = snapshot
            .requests
            .iter()
            .filter(|request| request.status == "requested")
            .count();
        let banner = self.object::<adw::Banner>("requests");
        banner.set_title(&std::format!(
            "{}: {pending}",
            text("linux_pending_requests")
        ));
        banner.set_revealed(pending != 0);
        self.object::<gtk::Stack>("root").set_visible_child_name(
            if snapshot.transfers.is_empty() {
                "welcome"
            } else {
                "main"
            },
        );
        let keys: Vec<_> = snapshot
            .transfers
            .iter()
            .map(|t| (t.transfer_id, t.direction.clone()))
            .collect();
        if self
            .rows
            .borrow()
            .iter()
            .map(|row| &row.key)
            .ne(keys.iter())
        {
            while let Some(child) = self.list.first_child() {
                self.list.remove(&child);
            }
            let mut rows = self.rows.borrow_mut();
            rows.clear();
            for key in keys {
                let row = adw::ActionRow::builder()
                    .activatable(true)
                    .title_lines(1)
                    .subtitle_lines(2)
                    .build();
                row.add_prefix(&gtk::Image::from_icon_name(if key.1 == "send" {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                }));
                let spinner = gtk::Spinner::new();
                row.add_suffix(&spinner);
                self.list.append(&row);
                rows.push(TransferRow { key, row, spinner });
            }
        }
        for (row, transfer) in self.rows.borrow().iter().zip(&snapshot.transfers) {
            row.row.set_title(&glib::markup_escape_text(
                transfer
                    .transfer_name
                    .as_deref()
                    .unwrap_or(&text("receive_unknown_transfer")),
            ));
            row.row
                .set_subtitle(&glib::markup_escape_text(&std::format!(
                    "{} · {}",
                    text(presentation::status_key(&transfer.status)),
                    glib::format_size(transfer.total_size)
                )));
            let busy = matches!(transfer.status.as_str(), "importing" | "receiving");
            row.spinner.set_visible(busy);
            row.spinner.set_spinning(busy);
            if self.selected.borrow().as_ref() == Some(&row.key) {
                self.list.select_row(Some(&row.row));
            }
        }
        self.object::<gtk::Stack>("list_stack")
            .set_visible_child_name(if snapshot.transfers.is_empty() {
                "empty"
            } else {
                "list"
            });
        self.render_details();
    }

    pub(super) fn render_details(self: &Rc<Self>) {
        let snapshot = self.snapshot.borrow();
        let selected = self.selected.borrow();
        let transfer = snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .transfers
                .iter()
                .find(|t| selected.as_ref() == Some(&(t.transfer_id, t.direction.clone())))
        });
        let fingerprint = if let Some(transfer) = transfer {
            let snapshot = snapshot.as_ref().unwrap();
            serde_json::to_string(&(
                transfer,
                snapshot
                    .requests
                    .iter()
                    .filter(|r| r.transfer_id == transfer.transfer_id)
                    .collect::<Vec<_>>(),
                snapshot
                    .artifacts
                    .iter()
                    .filter(|a| a.transfer_local_id == transfer.local_id)
                    .collect::<Vec<_>>(),
            ))
            .unwrap()
        } else {
            "empty".into()
        };
        if *self.detail_fingerprint.borrow() == fingerprint {
            return;
        }
        self.detail_fingerprint.replace(fingerprint);
        // Hand off focus before replacing selectable labels and action controls.
        // GTK can otherwise retain a removed descendant during the next layout.
        let had_focus = gtk::prelude::GtkWindowExt::focus(&self.window)
            .is_some_and(|focus| focus.is_ancestor(&self.details));
        if had_focus {
            gtk::prelude::GtkWindowExt::set_focus(&self.window, None::<&gtk::Widget>);
        }
        while let Some(child) = self.details.first_child() {
            self.details.remove(&child);
        }
        let Some(transfer) = transfer else {
            self.details.append(
                &adw::StatusPage::builder()
                    .title(text("linux_select_transfer"))
                    .description(text("linux_select_transfer_body"))
                    .icon_name("folder-download-symbolic")
                    .vexpand(true)
                    .build(),
            );
            return;
        };
        let id = transfer.transfer_id;
        let title = gtk::Label::builder()
            .label(
                transfer
                    .transfer_name
                    .as_deref()
                    .unwrap_or(&text("receive_unknown_transfer")),
            )
            .wrap(true)
            .xalign(0.0)
            .selectable(true)
            .build();
        title.add_css_class("title-1");
        self.details.append(&title);
        let state = gtk::Label::builder()
            .label(text(presentation::status_key(&transfer.status)))
            .xalign(0.0)
            .build();
        state.add_css_class("dim-label");
        self.details.append(&state);
        let summary = adw::PreferencesGroup::new();
        summary.add(&fact(
            "linux_direction",
            &text(if transfer.direction == "send" {
                "linux_outgoing"
            } else {
                "linux_incoming"
            }),
        ));
        summary.add(&fact("metadata_files", &transfer.file_count.to_string()));
        summary.add(&fact(
            "metadata_size",
            &glib::format_size(transfer.total_size),
        ));
        self.details.append(&summary);
        if let Some(ticket) = presentation::invitation(transfer) {
            let invitation_group = adw::PreferencesGroup::builder()
                .title(text("linux_invitation"))
                .build();
            let copy = adw::ActionRow::builder()
                .title(text("linux_copy_invitation"))
                .activatable(true)
                .build();
            copy.add_suffix(&gtk::Image::from_icon_name("edit-copy-symbolic"));
            let ticket_copy = ticket.to_owned();
            let weak = Rc::downgrade(self);
            copy.connect_activated(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.window.clipboard().set_text(&ticket_copy);
                    app.error("linux_invitation_copied");
                }
            });
            invitation_group.add(&copy);
            let save = adw::ActionRow::builder()
                .title(text("linux_save_invitation"))
                .activatable(true)
                .build();
            save.add_suffix(&gtk::Image::from_icon_name("document-save-symbolic"));
            let (ticket, name) = (
                ticket.to_owned(),
                invitation::file_name(transfer.transfer_name.as_deref().unwrap_or("VniDrop")),
            );
            let weak = Rc::downgrade(self);
            save.connect_activated(move |_| {
                let Some(app) = weak.upgrade() else {
                    return;
                };
                let (ticket, name) = (ticket.clone(), name.clone());
                glib::spawn_future_local(async move {
                    let picker = gtk::FileDialog::builder()
                        .title(text("linux_save_invitation"))
                        .initial_name(name)
                        .build();
                    match picker.save_future(Some(&app.window)).await {
                        Ok(file) => {
                            let result = file
                                .replace_contents_future(
                                    ticket.into_bytes(),
                                    None,
                                    false,
                                    gio::FileCreateFlags::PRIVATE,
                                )
                                .await;
                            match result {
                                Ok(_) => app.error("transfer_invitation_saved"),
                                Err(_) => app.error("error_filesystem"),
                            }
                        }
                        Err(error) if error.matches(gtk::DialogError::Dismissed) => (),
                        Err(_) => app.error("error_selection_failed"),
                    }
                });
            });
            invitation_group.add(&save);
            self.details.append(&invitation_group);
        }
        if transfer.direction == "send" {
            let receivers = adw::PreferencesGroup::builder()
                .title(text("transfer_receivers_title"))
                .build();
            let requests: Vec<_> = snapshot
                .as_ref()
                .unwrap()
                .requests
                .iter()
                .filter(|request| request.transfer_id == id)
                .collect();
            if requests.is_empty() {
                receivers.add(
                    &adw::ActionRow::builder()
                        .title(text("linux_no_receivers"))
                        .build(),
                );
            }
            for request in requests {
                let receiver = request
                    .receiver_name
                    .as_deref()
                    .or(request.receiver_device_name.as_deref())
                    .unwrap_or(&text("approval_nearby_device"))
                    .to_owned();
                let status = match request.status.as_str() {
                    "requested" => "transfer_receiver_requested",
                    "accepted" => "transfer_receiver_accepted",
                    "completed" => "transfer_receiver_completed",
                    "refused" => "transfer_receiver_refused",
                    "expired" => "transfer_receiver_expired",
                    "failed" => "transfer_receiver_failed",
                    _ => "transfer_receiver_unknown",
                };
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&receiver))
                    .subtitle(text(status))
                    .build();
                if request.status == "requested" {
                    row.set_tooltip_text(Some(&format(
                        "approval_request_body",
                        &[
                            ("receiver", &receiver),
                            ("transferName", &request.transfer_name),
                        ],
                    )));
                    let approve = gtk::Button::from_icon_name("object-select-symbolic");
                    approve.set_tooltip_text(Some(&text("button_approve")));
                    approve.set_valign(gtk::Align::Center);
                    let refuse = gtk::Button::from_icon_name("window-close-symbolic");
                    refuse.set_tooltip_text(Some(&text("button_refuse")));
                    refuse.set_valign(gtk::Align::Center);
                    for (button, accepted) in [(&approve, true), (&refuse, false)] {
                        let weak = Rc::downgrade(self);
                        let request_id = request.id.clone();
                        let row = row.downgrade();
                        button.connect_clicked(move |_| {
                            let Some(app) = weak.upgrade() else {
                                return;
                            };
                            let Some(row) = row.upgrade() else {
                                return;
                            };
                            row.set_sensitive(false);
                            let request = request_id.clone();
                            let row = row.clone();
                            app.dispatch(
                                move |session| {
                                    session.call(|core| {
                                        core.respond_receiver_request(request, accepted, None)
                                    })
                                },
                                move |app, result| {
                                    if let Err(key) = result {
                                        row.set_sensitive(true);
                                        app.error(key);
                                    }
                                    app.refresh();
                                },
                            );
                        });
                    }
                    row.add_suffix(&refuse);
                    row.add_suffix(&approve);
                }
                receivers.add(&row);
            }
            self.details.append(&receivers);
        }
        let artifacts = snapshot
            .as_ref()
            .unwrap()
            .artifacts
            .iter()
            .filter(|artifact| artifact.transfer_local_id == transfer.local_id)
            .collect::<Vec<_>>();
        if !artifacts.is_empty() {
            let files = adw::PreferencesGroup::builder()
                .title(text("storage_received_files"))
                .build();
            for artifact in artifacts {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&artifact.relative_path))
                    .activatable(true)
                    .build();
                row.add_suffix(&gtk::Image::from_icon_name("document-open-symbolic"));
                let file = gio::File::for_path(&artifact.locator);
                let weak = Rc::downgrade(self);
                row.connect_activated(move |_| {
                    let Some(app) = weak.upgrade() else {
                        return;
                    };
                    let file = file.clone();
                    glib::spawn_future_local(async move {
                        if gtk::FileLauncher::new(Some(&file))
                            .launch_future(Some(&app.window))
                            .await
                            .is_err()
                        {
                            app.error("linux_open_failed");
                        }
                    });
                });
                files.add(&row);
            }
            self.details.append(&files);
        }
        let active = presentation::is_active(transfer);
        let action = gtk::Button::with_label(&text(if active {
            if transfer.direction == "send" {
                "send_stop_sharing"
            } else {
                "button_cancel_receive"
            }
        } else {
            "linux_remove_history"
        }));
        action.add_css_class("destructive-action");
        action.set_halign(gtk::Align::Start);
        let weak = Rc::downgrade(self);
        action.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            glib::spawn_future_local(async move {
                let (title, body, action) = if active {
                    (
                        "linux_cancel_title",
                        "linux_cancel_body",
                        "button_cancel_receive",
                    )
                } else {
                    (
                        "linux_remove_title",
                        "linux_remove_body",
                        "linux_remove_history",
                    )
                };
                if !dialogs::confirm(&app.window, title, body, action).await {
                    return;
                }
                app.dispatch(
                    move |session| {
                        session.call(|core| {
                            if active {
                                core.cancel_transfer(id)
                            } else {
                                core.delete_transfer(id)
                            }
                        })
                    },
                    |app, result| {
                        if let Err(key) = result {
                            app.error(key);
                        }
                        app.refresh();
                    },
                );
            });
        });
        self.details.append(&action);
        if had_focus {
            self.details.child_focus(gtk::DirectionType::TabForward);
        }
    }
}
