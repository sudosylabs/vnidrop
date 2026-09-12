use super::{
    dialogs,
    i18n::{format, text},
    widgets::{icon_button, ProgressView},
    App,
};
use adw::prelude::*;
use gtk::glib;
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};
use vnidrop::{
    TargetedOfferResponse, TargetedTransferRole as Role, TargetedTransferState as State,
};
use vnidrop_gnome::{
    progress::Progress,
    targeted::{self, Action},
};

pub(super) struct DeviceTransfers {
    pub root: gtk::Box,
    peer: String,
    history: bool,
    destination: RefCell<Option<PathBuf>>,
    rendered: RefCell<String>,
    pub approvals: RefCell<HashMap<String, gtk::Button>>,
    progress: RefCell<HashMap<String, ProgressView>>,
}

#[derive(Clone, Copy)]
pub(super) enum Command {
    Accept,
    Decline,
    Transfer(Action),
}

impl App {
    fn targeted_command(self: &Rc<Self>, id: String, command: Command, directory: PathBuf) {
        let app = self.clone();
        glib::spawn_future_local(async move {
            if matches!(command, Command::Transfer(Action::Cancel | Action::Delete)) {
                if let Some(dialog) = app.window.visible_dialog() {
                    dialog.force_close();
                }
                let deleting = matches!(command, Command::Transfer(Action::Delete));
                if !dialogs::confirm(
                    &app.window,
                    if deleting {
                        "linux_remove_title"
                    } else {
                        "linux_cancel_title"
                    },
                    if deleting {
                        "linux_remove_body"
                    } else {
                        "linux_cancel_body"
                    },
                    if deleting {
                        "saved_devices_transfer_delete"
                    } else {
                        "linux_cancel_direct_transfer"
                    },
                )
                .await
                {
                    return;
                }
            }
            let key = if matches!(command, Command::Transfer(Action::Cancel)) {
                format!("cancel:{id}")
            } else {
                id.clone()
            };
            if !app.targeted_busy.borrow_mut().insert(key.clone()) {
                return;
            }
            app.clear_review_highlight();
            app.render();
            app.dispatch(
                move |session| {
                    session.call(|core| {
                        let directory = directory.to_string_lossy().into_owned();
                        match command {
                            Command::Accept | Command::Decline => {
                                match core.respond_to_targeted_offer(
                                    id,
                                    matches!(command, Command::Accept),
                                )? {
                                    TargetedOfferResponse::Approved { transfer_id } => {
                                        core.receive_targeted_transfer(transfer_id, directory)
                                    }
                                    TargetedOfferResponse::Declined
                                    | TargetedOfferResponse::AlreadySettled { .. } => Ok(()),
                                }
                            }
                            Command::Transfer(Action::Receive) => {
                                core.receive_targeted_transfer(id, directory)
                            }
                            Command::Transfer(Action::Resume) => {
                                core.resume_targeted_transfer(id, directory)
                            }
                            Command::Transfer(Action::Cancel) => core.cancel_targeted_transfer(id),
                            Command::Transfer(Action::Delete) => core.delete_targeted_transfer(id),
                        }
                    })
                },
                move |app, result| {
                    app.targeted_busy.borrow_mut().remove(&key);
                    if let Err(key) = result {
                        app.error(key);
                    }
                    app.refresh();
                },
            );
        });
    }

    fn show_device_history(self: &Rc<Self>, peer: String) {
        if self.window.visible_dialog().is_some() {
            return;
        }
        let (dialog, content) = dialogs::scrollable_content("linux_device_transfer_history");
        let mut panel = DeviceTransfers::new(peer);
        Rc::get_mut(&mut panel).unwrap().history = true;
        content.append(&panel.root);
        panel.render(self);
        let weak = Rc::downgrade(self);
        dialog.connect_closed(move |_| {
            if let Some(app) = weak.upgrade() {
                app.targeted_history.borrow_mut().take();
            }
        });
        self.targeted_history.replace(Some(panel));
        dialog.present(Some(&self.window));
    }
}

impl DeviceTransfers {
    pub fn new(peer: String) -> Rc<Self> {
        Rc::new(Self {
            root: gtk::Box::new(gtk::Orientation::Vertical, 12),
            peer,
            history: false,
            destination: RefCell::new(None),
            rendered: RefCell::new(String::new()),
            approvals: RefCell::new(HashMap::new()),
            progress: RefCell::new(HashMap::new()),
        })
    }

    pub fn render(self: &Rc<Self>, app: &Rc<App>) {
        let snapshot = app.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            return;
        };
        let offers: Vec<_> = snapshot
            .offers
            .iter()
            .filter(|o| o.sender_endpoint_id == self.peer && !self.history)
            .collect();
        let mut transfers: Vec<_> = snapshot
            .targeted
            .iter()
            .filter(|t| targeted::peer(t) == self.peer && t.state != State::Deleted)
            .collect();
        transfers.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        let directory = self
            .destination
            .borrow()
            .clone()
            .unwrap_or_else(|| app.preferences.borrow().receive_directory.clone());
        let busy = app.targeted_busy.borrow();
        let fingerprint = serde_json::to_string(&(
            &offers,
            transfers
                .iter()
                .map(|t| {
                    (
                        &t.id,
                        t.role,
                        &t.transfer_name,
                        t.state,
                        t.file_count,
                        t.total_size,
                    )
                })
                .collect::<Vec<_>>(),
            &*busy,
            &directory,
        ))
        .unwrap();
        if *self.rendered.borrow() == fingerprint {
            for transfer in &transfers {
                if let Some(progress) = self.progress.borrow().get(&transfer.id) {
                    progress.update(Some(&Progress {
                        label: targeted::status_key(transfer.state),
                        bytes: Some(transfer.verified_bytes),
                        total: Some(transfer.total_size),
                    }));
                }
            }
            return;
        }
        self.rendered.replace(fingerprint);
        let had_focus = gtk::prelude::RootExt::focus(&app.window)
            .is_some_and(|focus| focus.is_ancestor(&self.root));
        if had_focus {
            gtk::prelude::GtkWindowExt::set_focus(&app.window, None::<&gtk::Widget>);
        }
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
        self.approvals.borrow_mut().clear();
        self.progress.borrow_mut().clear();
        let title = gtk::Label::builder()
            .label(text("saved_devices_transfers_title"))
            .xalign(0.0)
            .build();
        title.add_css_class("heading");
        if !self.history {
            self.root.append(&title);
        }
        let can_pull = !offers.is_empty()
            || transfers.iter().any(|t| {
                targeted::actions(t)
                    .iter()
                    .any(|action| matches!(action, Action::Receive | Action::Resume))
            });
        if can_pull && !self.history {
            let folder = dialogs::fact(
                "preferences_receive_folder_title",
                &directory.display().to_string(),
            );
            let choose = gtk::Button::builder()
                .icon_name("folder-open-symbolic")
                .tooltip_text(text("linux_choose_destination"))
                .valign(gtk::Align::Center)
                .build();
            let (weak, weak_app) = (Rc::downgrade(self), Rc::downgrade(app));
            choose.connect_clicked(move |_| {
                let (Some(panel), Some(app)) = (weak.upgrade(), weak_app.upgrade()) else {
                    return;
                };
                glib::spawn_future_local(async move {
                    match gtk::FileDialog::builder()
                        .title(text("linux_choose_destination"))
                        .build()
                        .select_folder_future(Some(&app.window))
                        .await
                    {
                        Ok(folder) => {
                            if let Some(path) = folder.path() {
                                panel.destination.replace(Some(path));
                                panel.render(&app);
                            }
                        }
                        Err(error) if error.matches(gtk::DialogError::Dismissed) => {}
                        Err(_) => app.error("error_selection_failed"),
                    }
                });
            });
            folder.add_suffix(&choose);
            let group = adw::PreferencesGroup::new();
            group.add(&folder);
            self.root.append(&group);
        }
        for offer in offers {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
            card.add_css_class("card");
            card.add_css_class("review-target");
            if app.reviewed_offer.borrow().as_ref() == Some(&offer.transfer_id) {
                card.add_css_class("review-highlight");
                app.review_highlight.replace(Some(card.clone().upcast()));
            }
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&offer.transfer_name))
                .subtitle(format(
                    "linux_direct_transfer_files",
                    &[
                        ("count", &offer.file_count.to_string()),
                        ("size", &glib::format_size(offer.total_size)),
                    ],
                ))
                .title_lines(2)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name("document-send-symbolic"));
            let rows = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .build();
            rows.add_css_class("device-transfer-header");
            rows.append(&row);
            card.append(&rows);
            let actions = gtk::Box::builder()
                .spacing(12)
                .homogeneous(true)
                .margin_start(12)
                .margin_end(12)
                .margin_bottom(12)
                .build();
            for (key, icon, command) in [
                ("button_refuse", "window-close-symbolic", Command::Decline),
                ("button_approve", "emblem-ok-symbolic", Command::Accept),
            ] {
                let button = icon_button(key, icon);
                button.set_sensitive(!busy.contains(&offer.transfer_id));
                if matches!(command, Command::Accept) {
                    button.add_css_class("suggested-action");
                    self.approvals
                        .borrow_mut()
                        .insert(offer.transfer_id.clone(), button.clone());
                }
                let (weak, id, directory) = (
                    Rc::downgrade(app),
                    offer.transfer_id.clone(),
                    directory.clone(),
                );
                button.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.targeted_command(id.clone(), command, directory.clone());
                    }
                });
                actions.append(&button);
            }
            card.append(&actions);
            self.root.append(&card);
        }
        let mut finished = 0;
        let mut shown = 0;
        for transfer in transfers {
            let terminal = targeted::actions(transfer).contains(&Action::Delete);
            if terminal {
                finished += 1;
            }
            if terminal != self.history {
                continue;
            }
            if snapshot
                .offers
                .iter()
                .any(|offer| offer.transfer_id == transfer.id)
            {
                continue;
            }
            shown += 1;
            let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
            card.add_css_class("card");
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&transfer.transfer_name))
                .subtitle(format!(
                    "{} · {}",
                    text(if transfer.role == Role::Sender {
                        "linux_outgoing"
                    } else {
                        "linux_incoming"
                    }),
                    text(targeted::status_key(transfer.state))
                ))
                .title_lines(2)
                .subtitle_lines(2)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name(
                if transfer.role == Role::Sender {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                },
            ));
            let rows = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .build();
            rows.add_css_class("device-transfer-header");
            rows.append(&row);
            card.append(&rows);
            let size = gtk::Label::builder()
                .label(format(
                    "linux_direct_transfer_files",
                    &[
                        ("count", &transfer.file_count.to_string()),
                        ("size", &glib::format_size(transfer.total_size)),
                    ],
                ))
                .xalign(0.0)
                .margin_start(12)
                .margin_end(12)
                .wrap(true)
                .build();
            size.add_css_class("dim-label");
            card.append(&size);
            if transfer.role == Role::Receiver
                && matches!(
                    transfer.state,
                    State::Connecting | State::Transferring | State::Interrupted
                )
            {
                let progress = ProgressView::new();
                progress.root.set_margin_start(12);
                progress.root.set_margin_end(12);
                progress.update(Some(&Progress {
                    label: targeted::status_key(transfer.state),
                    bytes: Some(transfer.verified_bytes),
                    total: Some(transfer.total_size),
                }));
                card.append(&progress.root);
                self.progress
                    .borrow_mut()
                    .insert(transfer.id.clone(), progress);
            }
            let actions = gtk::Box::builder()
                .spacing(12)
                .margin_start(12)
                .margin_end(12)
                .margin_bottom(12)
                .build();
            for action in targeted::actions(transfer) {
                let (key, icon) = match action {
                    Action::Receive => {
                        ("saved_devices_transfer_receive", "folder-download-symbolic")
                    }
                    Action::Resume => (
                        "saved_devices_transfer_resume",
                        "media-playback-start-symbolic",
                    ),
                    Action::Cancel => ("saved_devices_transfer_cancel", "process-stop-symbolic"),
                    Action::Delete => ("saved_devices_transfer_delete", "edit-delete-symbolic"),
                };
                let button = icon_button(key, icon);
                button.set_sensitive(if action == Action::Cancel {
                    !busy.contains(&format!("cancel:{}", transfer.id))
                } else {
                    !busy.contains(&transfer.id)
                });
                if matches!(action, Action::Receive | Action::Resume) {
                    button.add_css_class("suggested-action");
                }
                let (weak, id, directory) =
                    (Rc::downgrade(app), transfer.id.clone(), directory.clone());
                button.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.targeted_command(
                            id.clone(),
                            Command::Transfer(action),
                            directory.clone(),
                        );
                    }
                });
                actions.append(&button);
            }
            card.append(&actions);
            self.root.append(&card);
        }
        if !self.history && finished > 0 {
            let row = adw::ActionRow::builder()
                .title(text("linux_device_transfer_history"))
                .subtitle(format(
                    "linux_receiver_history_count",
                    &[("count", &finished.to_string())],
                ))
                .activatable(true)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name("document-open-recent-symbolic"));
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            let (weak, peer) = (Rc::downgrade(app), self.peer.clone());
            row.connect_activated(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.show_device_history(peer.clone());
                }
            });
            let group = adw::PreferencesGroup::new();
            group.add(&row);
            self.root.append(&group);
        } else if shown == 0 && self.approvals.borrow().is_empty() {
            let empty = gtk::Label::builder()
                .label(text("saved_devices_transfer_empty"))
                .xalign(0.0)
                .wrap(true)
                .build();
            empty.add_css_class("dim-label");
            self.root.append(&empty);
        }
        if had_focus {
            let reviewed = app
                .reviewed_offer
                .borrow()
                .as_ref()
                .and_then(|id| self.approvals.borrow().get(id).cloned());
            if let Some(button) = reviewed {
                app.focus_review(&button);
            } else {
                self.root.child_focus(gtk::DirectionType::TabForward);
            }
        }
    }
}
