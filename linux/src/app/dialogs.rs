use std::{cell::RefCell, path::PathBuf, rc::Rc};

use adw::prelude::*;
use gtk::{gio, glib};
use vnidrop_gnome::invitation;

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

pub(super) fn content(title: &str) -> (adw::Dialog, gtk::Box, adw::HeaderBar) {
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

pub(super) fn scrollable_content(title: &str) -> (adw::Dialog, gtk::Box) {
    let (dialog, rows, _) = content(title);
    rows.ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
        .unwrap()
        .set_propagate_natural_height(false);
    (dialog, rows)
}

pub fn fact(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(text(title))
        .subtitle(glib::markup_escape_text(value))
        .subtitle_selectable(true)
        .build()
}

impl App {
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
        if self.reviewing.get()
            || self.session.borrow().is_none()
            || self.closing.get()
            || self.composer.borrow().is_some()
            || self.window.visible_dialog().is_some()
        {
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
                    app.show_transfers();
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
