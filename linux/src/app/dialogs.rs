use std::{cell::RefCell, path::PathBuf, rc::Rc};

use adw::prelude::*;
use gtk::{gio, glib};
use vnidrop::{ShareMetadataInput, TransferAccessMode};
use vnidrop_gnome::{
    draft::{self, Preparation},
    invitation,
};

use super::{i18n::text, App};

pub async fn confirm(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    body: &str,
    action: &str,
) -> bool {
    let dialog = adw::AlertDialog::builder()
        .heading(text(heading))
        .body(text(body))
        .build();
    dialog.add_responses(&[
        ("cancel", &text("button_cancel")),
        ("confirm", &text(action)),
    ]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("cancel"));
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
    dialog.choose_future(Some(parent)).await == "confirm"
}

fn content(title: &str) -> (adw::Dialog, gtk::Box, adw::HeaderBar) {
    let header = adw::HeaderBar::new();
    let rows = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&rows)
        .build();
    let toolbar = adw::ToolbarView::builder().content(&scroll).build();
    toolbar.add_top_bar(&header);
    let dialog = adw::Dialog::builder()
        .title(text(title))
        .content_width(480)
        .content_height(520)
        .child(&toolbar)
        .build();
    (dialog, rows, header)
}

pub fn fact(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(text(title))
        .subtitle(glib::markup_escape_text(value))
        .subtitle_selectable(true)
        .build()
}

impl App {
    pub(super) fn pick_sources(self: &Rc<Self>, folder: bool) {
        let app = self.clone();
        glib::spawn_future_local(async move {
            let picker = gtk::FileDialog::builder()
                .title(text(if folder {
                    "linux_send_folder"
                } else {
                    "linux_send_files"
                }))
                .build();
            let result = if folder {
                picker
                    .select_folder_future(Some(&app.window))
                    .await
                    .map(|file| vec![file])
            } else {
                picker
                    .open_multiple_future(Some(&app.window))
                    .await
                    .map(|files| {
                        (0..files.n_items())
                            .filter_map(|i| files.item(i).and_downcast::<gio::File>())
                            .collect()
                    })
            };
            match result {
                Ok(files) => app.compose(files),
                Err(error) if error.matches(gtk::DialogError::Dismissed) => (),
                Err(_) => app.error("error_selection_failed"),
            }
        });
    }

    pub(super) fn compose(self: &Rc<Self>, files: Vec<gio::File>) {
        let paths: Option<Vec<_>> = files.iter().map(gio::File::path).collect();
        let Some(paths) = paths else {
            self.error("linux_local_files_only");
            return;
        };
        self.dispatch(
            move |_| draft::sources(paths),
            |app, result| {
                let sources = match result {
                    Ok(sources) => sources,
                    Err(key) => {
                        app.error(key);
                        return;
                    }
                };
                let (dialog, rows, header) = content("send_new_transfer_title");
                let name = adw::EntryRow::builder()
                    .title(text("field_transfer_name"))
                    .build();
                name.set_text(
                    sources
                        .first()
                        .and_then(|source| source.display_name.as_deref())
                        .filter(|_| sources.len() == 1)
                        .unwrap_or(""),
                );
                let naming = adw::PreferencesGroup::new();
                naming.add(&name);
                rows.append(&naming);
                let selection = adw::PreferencesGroup::builder()
                    .title(text("metadata_files"))
                    .build();
                for source in &sources {
                    let row = adw::ActionRow::builder()
                        .title(glib::markup_escape_text(
                            source.display_name.as_deref().unwrap_or(""),
                        ))
                        .build();
                    row.add_prefix(&gtk::Image::from_icon_name(if source.is_directory {
                        "folder-symbolic"
                    } else {
                        "text-x-generic-symbolic"
                    }));
                    selection.add(&row);
                }
                rows.append(&selection);
                let access = adw::PreferencesGroup::new();
                let choices = gtk::StringList::new(&[
                    &text("send_access_approval"),
                    &text("send_access_anyone"),
                ]);
                let policy = adw::ComboRow::builder()
                    .title(text("send_access_title"))
                    .model(&choices)
                    .use_subtitle(true)
                    .build();
                let warning = gtk::Label::builder()
                    .label(text("send_access_anyone_warning"))
                    .wrap(true)
                    .xalign(0.0)
                    .visible(false)
                    .build();
                warning.add_css_class("caption");
                let warning_clone = warning.clone();
                policy.connect_selected_notify(move |policy| {
                    warning_clone.set_visible(policy.selected() == 1)
                });
                access.add(&policy);
                rows.append(&access);
                rows.append(&warning);
                let error = gtk::Label::builder()
                    .wrap(true)
                    .xalign(0.0)
                    .visible(false)
                    .build();
                error.add_css_class("error");
                rows.append(&error);
                let create = gtk::Button::with_label(&text("linux_create_invitation"));
                create.add_css_class("suggested-action");
                header.pack_end(&create);
                let cancel = gtk::Button::with_label(&text("button_cancel"));
                cancel.set_visible(false);
                header.pack_start(&cancel);
                let current: Rc<RefCell<Option<std::sync::Arc<Preparation>>>> = Rc::default();
                let weak = Rc::downgrade(&app);
                let preparation = current.clone();
                cancel.connect_clicked(move |button| {
                    let (Some(app), Some(preparation)) =
                        (weak.upgrade(), preparation.borrow().clone())
                    else {
                        return;
                    };
                    button.set_sensitive(false);
                    preparation.request_cancel();
                    app.dispatch(
                        move |session| {
                            preparation.cancel(&session);
                            Ok(())
                        },
                        |app, _| app.refresh(),
                    );
                });
                let weak = Rc::downgrade(&app);
                let dialog_copy = dialog.downgrade();
                create.connect_clicked(move |button| {
                    let Some(app) = weak.upgrade() else {
                        return;
                    };
                    let Some(dialog_copy) = dialog_copy.upgrade() else {
                        return;
                    };
                    let preparation = Preparation::new();
                    current.replace(Some(preparation.clone()));
                    button.set_sensitive(false);
                    cancel.set_visible(true);
                    cancel.set_sensitive(true);
                    dialog_copy.set_can_close(false);
                    name.set_sensitive(false);
                    policy.set_sensitive(false);
                    error.set_visible(false);
                    button.set_label(&text("button_sharing_file"));
                    let sources = sources.clone();
                    let metadata = ShareMetadataInput {
                        transfer_id: preparation.id,
                        transfer_name: (!name.text().trim().is_empty())
                            .then(|| name.text().trim().to_owned()),
                        sender_name: Some(app.preferences.borrow().username.clone()),
                        access_mode: if policy.selected() == 0 {
                            TransferAccessMode::ApprovalRequired
                        } else {
                            TransferAccessMode::Public
                        },
                    };
                    let (dialog, button, error, cancel, name, policy, current) = (
                        dialog_copy.clone(),
                        button.clone(),
                        error.clone(),
                        cancel.clone(),
                        name.clone(),
                        policy.clone(),
                        current.clone(),
                    );
                    let completion = preparation.clone();
                    app.dispatch(
                        move |session| preparation.run(&session, sources, metadata),
                        move |app, result| {
                            let result = if completion.is_cancelled() {
                                Err("progress_cancelled")
                            } else {
                                result
                            };
                            current.replace(None);
                            dialog.set_can_close(true);
                            button.set_sensitive(true);
                            button.set_label(&text("linux_create_invitation"));
                            cancel.set_visible(false);
                            name.set_sensitive(true);
                            policy.set_sensitive(true);
                            match result {
                                Ok(share) => {
                                    app.selected
                                        .replace(Some((share.transfer_id, "send".into())));
                                    dialog.close();
                                    app.split.set_show_content(true);
                                }
                                Err(key) => {
                                    #[cfg(test)]
                                    eprintln!("GTK preparation: {key}");
                                    error.set_text(&text(key));
                                    error.set_visible(true);
                                }
                            }
                            app.refresh();
                        },
                    );
                });
                dialog.present(Some(&app.window));
            },
        );
    }

    pub(super) fn pick_invitation(self: &Rc<Self>) {
        let app = self.clone();
        glib::spawn_future_local(async move {
            let filter = gtk::FileFilter::new();
            filter.set_name(Some(&text("linux_invitation")));
            filter.add_suffix("vnd");
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            let picker = gtk::FileDialog::builder()
                .title(text("linux_open_invitation"))
                .filters(&filters)
                .default_filter(&filter)
                .build();
            match picker.open_future(Some(&app.window)).await {
                Ok(file) => app.enqueue_invitation(file),
                Err(error) if error.matches(gtk::DialogError::Dismissed) => (),
                Err(_) => app.error("error_selection_failed"),
            }
        });
    }

    pub(super) fn enqueue_invitation(self: &Rc<Self>, file: gio::File) {
        if self.pending.borrow().len() >= 16 {
            self.error("error_invalid_input");
            return;
        }
        self.pending.borrow_mut().push_back(file);
        self.pump_invitations();
    }

    pub(super) fn pump_invitations(self: &Rc<Self>) {
        if self.reviewing.get() || self.session.borrow().is_none() || self.closing.get() {
            return;
        }
        let Some(file) = self.pending.borrow_mut().pop_front() else {
            return;
        };
        self.reviewing.set(true);
        let path = file.path();
        self.dispatch(
            move |session| {
                let path = path.ok_or("linux_local_files_only")?;
                let ticket = invitation::read(&path)?;
                let inspection = session.call(|core| core.inspect_ticket(ticket.clone()))?;
                Ok((ticket, inspection))
            },
            |app, result| {
                let (ticket, inspection) = match result {
                    Ok(value) => value,
                    Err(key) => {
                        app.reviewing.set(false);
                        app.error(key);
                        app.pump_invitations();
                        return;
                    }
                };
                let (dialog, rows, header) = content("receive_review_title");
                let metadata = inspection.metadata;
                let group = adw::PreferencesGroup::new();
                group.add(&fact("field_transfer_name", &metadata.transfer_name));
                if let Some(sender) = &metadata.sender_name {
                    group.add(&fact("field_sender_name", sender));
                }
                group.add(&fact("metadata_files", &metadata.file_count.to_string()));
                group.add(&fact(
                    "metadata_size",
                    &glib::format_size(metadata.total_size),
                ));
                rows.append(&group);
                let folder: Rc<RefCell<PathBuf>> = Rc::new(RefCell::new(
                    app.preferences.borrow().receive_directory.clone(),
                ));
                let destination = adw::PreferencesGroup::new();
                let row = fact(
                    "preferences_receive_folder_title",
                    &folder.borrow().display().to_string(),
                );
                let choose = gtk::Button::from_icon_name("folder-open-symbolic");
                choose.set_tooltip_text(Some(&text("linux_choose_destination")));
                choose.set_valign(gtk::Align::Center);
                row.add_suffix(&choose);
                destination.add(&row);
                rows.append(&destination);
                let weak = Rc::downgrade(&app);
                let selected_folder = folder.clone();
                choose.connect_clicked(move |_| {
                    let Some(app) = weak.upgrade() else {
                        return;
                    };
                    let (folder, row) = (selected_folder.clone(), row.clone());
                    glib::spawn_future_local(async move {
                        if let Some(path) = app.choose_folder().await {
                            row.set_subtitle(&glib::markup_escape_text(
                                &path.display().to_string(),
                            ));
                            folder.replace(path);
                        }
                    });
                });
                let receive = gtk::Button::with_label(&text("button_receive_files"));
                receive.add_css_class("suggested-action");
                header.pack_end(&receive);
                let weak = Rc::downgrade(&app);
                let dialog_copy = dialog.downgrade();
                receive.connect_clicked(move |button| {
                    let Some(app) = weak.upgrade() else {
                        return;
                    };
                    let Some(dialog_copy) = dialog_copy.upgrade() else {
                        return;
                    };
                    let Some(path) = folder.borrow().to_str().map(str::to_owned) else {
                        app.error("linux_local_files_only");
                        return;
                    };
                    button.set_sensitive(false);
                    let receiver = app.preferences.borrow().username.clone();
                    let ticket = ticket.clone();
                    app.selected
                        .replace(Some((metadata.transfer_id, "receive".into())));
                    app.dispatch(
                        move |session| {
                            session.call(|core| core.receive(ticket, path, Some(receiver)))
                        },
                        |app, result| {
                            if let Err(key) = result {
                                if key != "progress_cancelled" {
                                    app.error(key);
                                }
                            }
                            app.refresh();
                        },
                    );
                    dialog_copy.close();
                    app.split.set_show_content(true);
                    app.refresh();
                });
                let weak = Rc::downgrade(&app);
                dialog.connect_closed(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.reviewing.set(false);
                        app.pump_invitations();
                    }
                });
                dialog.present(Some(&app.window));
            },
        );
    }

    async fn choose_folder(self: &Rc<Self>) -> Option<PathBuf> {
        let picker = gtk::FileDialog::builder()
            .title(text("linux_choose_destination"))
            .build();
        match picker.select_folder_future(Some(&self.window)).await {
            Ok(file) => match file.path() {
                Some(path) => Some(path),
                None => {
                    self.error("linux_local_files_only");
                    None
                }
            },
            Err(error) if error.matches(gtk::DialogError::Dismissed) => None,
            Err(_) => {
                self.error("error_selection_failed");
                None
            }
        }
    }

    pub(super) fn show_preferences(self: &Rc<Self>) {
        let dialog = adw::PreferencesDialog::builder()
            .title(text("preferences_title"))
            .build();
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();
        let preferences = self.preferences.borrow().clone();
        let username = adw::EntryRow::builder()
            .title(text("field_username"))
            .text(&preferences.username)
            .show_apply_button(true)
            .build();
        let weak = Rc::downgrade(self);
        username.connect_apply(move |entry| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let mut prefs = app.preferences.borrow().clone();
            prefs.username = entry.text().trim().into();
            app.save_preferences(prefs);
        });
        group.add(&username);
        let folder = fact(
            "preferences_receive_folder_title",
            &preferences.receive_directory.display().to_string(),
        );
        folder.set_activatable(true);
        folder.add_suffix(&gtk::Image::from_icon_name("folder-open-symbolic"));
        let weak = Rc::downgrade(self);
        folder.connect_activated(move |row| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let row = row.clone();
            glib::spawn_future_local(async move {
                if let Some(path) = app.choose_folder().await {
                    let mut preferences = app.preferences.borrow().clone();
                    preferences.receive_directory = path;
                    row.set_subtitle(&glib::markup_escape_text(
                        &preferences.receive_directory.display().to_string(),
                    ));
                    app.save_preferences(preferences);
                }
            });
        });
        group.add(&folder);
        let themes = gtk::StringList::new(&[
            &text("linux_system"),
            &text("linux_light"),
            &text("linux_dark"),
        ]);
        let theme = adw::ComboRow::builder()
            .title(text("appearance_title"))
            .model(&themes)
            .selected(match preferences.theme.as_str() {
                "Light" => 1,
                "Dark" => 2,
                _ => 0,
            })
            .build();
        let weak = Rc::downgrade(self);
        theme.connect_selected_notify(move |row| {
            if let Some(app) = weak.upgrade() {
                let mut prefs = app.preferences.borrow().clone();
                prefs.theme = ["System", "Light", "Dark"][row.selected() as usize].into();
                app.save_preferences(prefs);
            }
        });
        group.add(&theme);
        page.add(&group);
        let network = adw::PreferencesGroup::new();
        let mode = match preferences.network.mode {
            vnidrop::CoreRelayMode::Automatic => "linux_network_automatic",
            vnidrop::CoreRelayMode::LocalOnly => "relay_mode_local_only",
            vnidrop::CoreRelayMode::StrictCustom => "relay_mode_custom",
            vnidrop::CoreRelayMode::CustomWithDirectFallback => "relay_mode_custom_direct_fallback",
        };
        network.add(&fact("linux_network_policy", &text(mode)));
        page.add(&network);
        dialog.add(&page);
        dialog.present(Some(&self.window));
    }

    fn save_preferences(self: &Rc<Self>, preferences: vnidrop_gnome::preferences::Preferences) {
        if preferences.username.trim().is_empty() {
            self.error("error_invalid_input");
            return;
        }
        self.preferences.replace(preferences.clone());
        self.apply_appearance();
        self.preference_queue.borrow_mut().push_back(preferences);
        if self.saving_preferences.replace(true) {
            return;
        }
        let app = self.clone();
        let hold = self.application.hold();
        glib::spawn_future_local(async move {
            loop {
                let Some(preferences) = app.preference_queue.borrow_mut().pop_front() else {
                    break;
                };
                let path = app.profile.clone();
                if let Err(key) = gio::spawn_blocking(move || preferences.save(&path))
                    .await
                    .unwrap_or(Err("error_generic"))
                {
                    app.error(key);
                }
            }
            app.saving_preferences.set(false);
            drop(hold);
        });
    }
}
