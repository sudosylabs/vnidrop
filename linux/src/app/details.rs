use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use vnidrop_gnome::{invitation, presentation, progress};

use super::{
    dialogs::{self, fact},
    i18n::{format, text},
    widgets::ProgressView,
    App,
};

pub(super) struct TransferRow {
    key: (u64, String),
    row: adw::ActionRow,
    spinner: gtk::Spinner,
    icon: gtk::Image,
    bar: gtk::ProgressBar,
}

impl App {
    pub(super) fn render(self: &Rc<Self>) {
        if self.reconfiguring.get() {
            return;
        }
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            return;
        };
        if self.reviewed_request.borrow().as_ref().is_some_and(|id| {
            !snapshot
                .requests
                .iter()
                .any(|request| &request.id == id && request.status == "requested")
        }) {
            self.clear_review_highlight();
        }
        if self
            .reviewed_offer
            .borrow()
            .as_ref()
            .is_some_and(|id| !snapshot.offers.iter().any(|offer| &offer.transfer_id == id))
        {
            self.clear_review_highlight();
        }
        self.schedule_device_expiry(
            snapshot
                .devices
                .eligible
                .iter()
                .map(|e| e.expires_at)
                .filter(|expiry| *expiry > glib::real_time() / 1000)
                .min(),
        );
        let attention = snapshot
            .devices
            .list(glib::real_time() / 1000)
            .iter()
            .filter(|d| d.needs_attention())
            .count()
            + snapshot.offers.len();
        self.object::<gtk::Label>("device_requests_title")
            .set_label(&format(
                "linux_device_attention",
                &[("count", &attention.to_string())],
            ));
        self.object::<gtk::Revealer>("device_requests")
            .set_reveal_child(attention != 0);
        if let Some(history) = self.targeted_history.borrow().as_ref() {
            history.render(self);
        }
        let devices = self.devices.borrow().clone();
        if let Some(devices) = devices {
            devices.render(self);
        }
        let pending = snapshot
            .requests
            .iter()
            .filter(|request| request.status == "requested")
            .count();
        self.object::<gtk::Label>("requests_title")
            .set_label(&format(
                "linux_transfer_attention",
                &[("count", &pending.to_string())],
            ));
        self.object::<gtk::Revealer>("requests")
            .set_reveal_child(pending != 0);
        self.object::<gtk::Revealer>("attention")
            .set_reveal_child(pending != 0 || attention != 0);
        self.object::<gtk::Stack>("root")
            .set_visible_child_name(if self.showing_devices.get() {
                "devices"
            } else if snapshot.transfers.is_empty() {
                "welcome"
            } else {
                "main"
            });
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
                let icon = gtk::Image::from_icon_name(if key.1 == "send" {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                });
                icon.set_pixel_size(32);
                row.add_prefix(&icon);
                let spinner = gtk::Spinner::new();
                row.add_suffix(&spinner);
                let bar = gtk::ProgressBar::builder()
                    .width_request(80)
                    .valign(gtk::Align::Center)
                    .build();
                row.add_suffix(&bar);
                self.list.append(&row);
                rows.push(TransferRow {
                    key,
                    row,
                    spinner,
                    icon,
                    bar,
                });
            }
        }
        for (row, transfer) in self.rows.borrow().iter().zip(&snapshot.transfers) {
            if row.key.1 == "send" {
                if let Some(texture) = self.previews.borrow().get(&row.key.0) {
                    row.icon.set_paintable(Some(texture));
                } else {
                    row.icon.set_icon_name(Some("go-up-symbolic"));
                }
            }
            row.row.set_title(&glib::markup_escape_text(
                transfer
                    .transfer_name
                    .as_deref()
                    .unwrap_or(&text("receive_unknown_transfer")),
            ));
            let live = progress::for_transfer(&snapshot.events, transfer).or_else(|| {
                (transfer.direction == "send" && transfer.status == "sharing")
                    .then(|| {
                        snapshot
                            .requests
                            .iter()
                            .filter(|request| request.transfer_id == transfer.transfer_id)
                            .find_map(|request| {
                                progress::for_receiver(
                                    &snapshot.events,
                                    request,
                                    transfer.total_size,
                                )
                            })
                    })
                    .flatten()
            });
            row.row
                .set_subtitle(&glib::markup_escape_text(&std::format!(
                    "{} · {} · {}",
                    text(
                        live.as_ref()
                            .map(|progress| progress.label)
                            .unwrap_or_else(|| presentation::status_key(&transfer.status))
                    ),
                    format(
                        "transfer_file_count",
                        &[("count", &transfer.file_count.to_string())]
                    ),
                    glib::format_size(transfer.total_size)
                )));
            let fraction = live.as_ref().and_then(progress::Progress::fraction);
            row.bar.set_visible(fraction.is_some());
            row.bar.set_fraction(fraction.unwrap_or(0.0));
            row.bar
                .update_property(&[gtk::accessible::Property::Label(&row.row.title())]);
            let busy = fraction.is_none()
                && (matches!(transfer.status.as_str(), "importing" | "receiving")
                    || live
                        .as_ref()
                        .is_some_and(|progress| progress.label != "progress_interrupted"));
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
        let qr = self.qr.borrow().clone();
        if let Some(qr) = qr {
            qr.refresh(snapshot);
        }
        if let Some(view) = self.detail_progress.borrow().as_ref() {
            let selected = self.selected.borrow();
            let live = snapshot
                .transfers
                .iter()
                .find(|transfer| {
                    selected.as_ref() == Some(&(transfer.transfer_id, transfer.direction.clone()))
                })
                .and_then(|transfer| progress::for_transfer(&snapshot.events, transfer));
            view.update(live.as_ref());
        }
        for (id, view) in self.receiver_progress.borrow().iter() {
            let live = snapshot
                .requests
                .iter()
                .find(|request| request.id == *id)
                .and_then(|request| {
                    snapshot
                        .transfers
                        .iter()
                        .find(|transfer| {
                            transfer.transfer_id == request.transfer_id
                                && transfer.direction == "send"
                                && transfer.status == "sharing"
                        })
                        .and_then(|transfer| {
                            progress::for_receiver(&snapshot.events, request, transfer.total_size)
                        })
                });
            view.update(live.as_ref());
        }
        if let Some(history) = self.receiver_history.borrow().as_ref() {
            history.render(&snapshot.requests);
        }
        if let Some(activity) = self.activity.borrow().as_ref() {
            activity.render(&snapshot.events);
        }
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
        self.review_actions.borrow_mut().clear();
        self.detail_progress.borrow_mut().take();
        self.receiver_progress.borrow_mut().clear();
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
        if transfer.direction == "receive"
            && matches!(transfer.status.as_str(), "failed" | "cancelled")
        {
            if let Some(ticket) = transfer.ticket.clone().or_else(|| {
                self.receive_drafts
                    .borrow()
                    .get(&id)
                    .map(|d| d.ticket.clone())
            }) {
                let retry = super::widgets::icon_button("button_retry", "view-refresh-symbolic");
                let weak = Rc::downgrade(self);
                retry.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.review_saved_invitation(ticket.clone());
                    }
                });
                self.details.append(&retry);
            }
        }
        if transfer.direction == "send" {
            if let Some(texture) = self.previews.borrow().get(&id) {
                let picture = gtk::Picture::for_paintable(texture);
                picture.set_size_request(128, 128);
                picture.set_halign(gtk::Align::Center);
                picture.set_content_fit(gtk::ContentFit::Contain);
                self.details.append(&picture);
            }
        }
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
        let live = ProgressView::new();
        live.update(progress::for_transfer(&snapshot.as_ref().unwrap().events, transfer).as_ref());
        self.details.append(&live.root);
        self.detail_progress.replace(Some(live));
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
        let activity = adw::ActionRow::builder()
            .title(text("transfer_activity_title"))
            .subtitle(text("transfer_activity_description"))
            .activatable(true)
            .build();
        activity.add_prefix(&gtk::Image::from_icon_name("document-open-recent-symbolic"));
        activity.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let weak = Rc::downgrade(self);
        let direction = transfer.direction.clone();
        activity.connect_activated(move |_| {
            if let Some(app) = weak.upgrade() {
                app.show_activity(id, direction.clone());
            }
        });
        let activity_group = adw::PreferencesGroup::new();
        activity_group.add(&activity);
        self.details.append(&activity_group);
        if let Some(ticket) = presentation::invitation(transfer) {
            let invitation_group = adw::PreferencesGroup::builder()
                .title(text("linux_invitation"))
                .build();
            let qr = adw::ActionRow::builder()
                .title(text("linux_show_qr"))
                .activatable(true)
                .build();
            qr.add_suffix(&gtk::Image::from_icon_name("qr-code-symbolic"));
            let transfer_id = transfer.transfer_id;
            let weak = Rc::downgrade(self);
            qr.connect_activated(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.show_qr(transfer_id);
                }
            });
            invitation_group.add(&qr);
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
            let finished = requests
                .iter()
                .filter(|request| super::receivers::is_finished(&request.status))
                .count();
            if finished > 0 {
                let history = adw::ActionRow::builder()
                    .title(text("linux_receiver_history"))
                    .subtitle(format(
                        "linux_receiver_history_count",
                        &[("count", &finished.to_string())],
                    ))
                    .activatable(true)
                    .build();
                history.add_prefix(&gtk::Image::from_icon_name("document-open-recent-symbolic"));
                history.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
                let weak = Rc::downgrade(self);
                history.connect_activated(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.show_receiver_history(id);
                    }
                });
                receivers.add(&history);
            }
            let receiver_list = gtk::Box::new(gtk::Orientation::Vertical, 12);
            if finished > 0 && finished < requests.len() {
                receiver_list.set_margin_top(12);
            }
            receivers.add(&receiver_list);
            for request in requests
                .into_iter()
                .filter(|request| !super::receivers::is_finished(&request.status))
            {
                let receiver = request
                    .receiver_name
                    .as_deref()
                    .or(request.receiver_device_name.as_deref())
                    .unwrap_or(&text("approval_nearby_device"))
                    .to_owned();
                let status = super::receivers::status_key(&request.status);
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&receiver))
                    .subtitle(text(status))
                    .title_lines(2)
                    .subtitle_lines(2)
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
                            app.clear_review_highlight();
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
                    self.review_actions
                        .borrow_mut()
                        .insert(request.id.clone(), approve);
                }
                let live = ProgressView::new();
                live.root.set_margin_start(12);
                live.root.set_margin_end(12);
                live.root.set_margin_bottom(12);
                live.update(
                    progress::for_receiver(
                        &snapshot.as_ref().unwrap().events,
                        request,
                        transfer.total_size,
                    )
                    .as_ref(),
                );
                let receiver = gtk::Box::new(gtk::Orientation::Vertical, 0);
                receiver.add_css_class("card");
                receiver.add_css_class("review-target");
                if request.status == "requested"
                    && self.reviewed_request.borrow().as_ref() == Some(&request.id)
                {
                    receiver.add_css_class("review-highlight");
                    self.review_highlight
                        .replace(Some(receiver.clone().upcast()));
                }
                receiver.append(&row);
                receiver.append(&live.root);
                receiver_list.append(&receiver);
                self.receiver_progress
                    .borrow_mut()
                    .push((request.id.clone(), live));
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
                let (content_type, _) =
                    gio::content_type_guess(Some(&artifact.relative_path), None);
                row.add_prefix(&gtk::Image::from_gicon(
                    &gio::content_type_get_symbolic_icon(&content_type),
                ));
                let file = gio::File::for_path(&artifact.locator);
                let reveal = gtk::Button::from_icon_name("folder-open-symbolic");
                reveal.set_tooltip_text(Some(&text("button_show_in_files")));
                reveal.set_valign(gtk::Align::Center);
                let reveal_file = file.clone();
                let weak = Rc::downgrade(self);
                reveal.connect_clicked(move |_| {
                    let Some(app) = weak.upgrade() else {
                        return;
                    };
                    let file = reveal_file.clone();
                    glib::spawn_future_local(async move {
                        if gtk::FileLauncher::new(Some(&file))
                            .open_containing_folder_future(Some(&app.window))
                            .await
                            .is_err()
                        {
                            app.error("linux_open_failed");
                        }
                    });
                });
                row.add_suffix(&reveal);

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
