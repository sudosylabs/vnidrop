use super::{
    dialogs::{self, fact},
    i18n::text,
    widgets::icon_button,
    App,
};
use adw::prelude::*;
use gtk::{gio, glib};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use vnidrop::{CoreRelayMode as Mode, CoreStorageUsage};
use vnidrop_gnome::reconfiguration::Maintenance;
use vnidrop_gnome::{preferences::Preferences, session::Session};

impl App {
    pub(super) fn attach_session(self: &Rc<Self>, session: Arc<Session>) {
        let changes = session.changes();
        self.session.replace(Some(session.clone()));
        self.refreshing.set(false);
        self.refresh_again.set(false);
        self.set_ready(true);
        self.refresh();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while changes.recv().await.is_ok() {
                glib::timeout_future(Duration::from_millis(120)).await;
                let Some(app) = weak.upgrade() else {
                    break;
                };
                if app.closing.get()
                    || !app
                        .session
                        .borrow()
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &session))
                {
                    break;
                }
                app.refresh();
            }
        });
    }

    fn reconfigure(self: &Rc<Self>, next: Preferences, maintenance: Maintenance) {
        if self.reconfiguring.get()
            || self.saving_preferences.get()
            || self.busy.get() > usize::from(self.refreshing.get()) + self.settings_reads.get()
        {
            self.error("relay_apply_active_transfers");
            return;
        }
        if self
            .snapshot
            .borrow()
            .as_ref()
            .is_some_and(|s| s.has_obligations())
        {
            self.error("relay_apply_active_transfers");
            return;
        }
        self.reconfiguring.set(true);
        self.set_ready(false);
        self.object::<gtk::Stack>("root")
            .set_visible_child_name("startup");
        self.object::<adw::StatusPage>("startup")
            .set_title(&text(match maintenance {
                Maintenance::None => "relay_applying",
                Maintenance::Cache => "storage_clearing_transfer_cache",
                Maintenance::Temporary => "storage_cleaning",
            }));
        self.object::<adw::StatusPage>("startup")
            .set_description(Some(&text("app_starting")));
        self.object::<gtk::Spinner>("spinner").set_visible(true);
        self.object::<gtk::Button>("retry").set_visible(false);
        self.object::<gtk::Button>("report_error")
            .set_visible(false);
        let app = self.clone();
        let hold = self.application.hold();
        glib::spawn_future_local(async move {
            while app.busy.get() != 0 {
                glib::timeout_future(Duration::from_millis(20)).await;
            }
            let session = app.session.borrow_mut().take();
            if let Some(session) = session {
                let (profile, previous) = (app.profile.clone(), app.preferences.borrow().clone());
                let result = gio::spawn_blocking(move || {
                    vnidrop_gnome::reconfiguration::apply(
                        session,
                        &profile,
                        previous,
                        next,
                        maintenance,
                    )
                })
                .await;
                app.reconfiguring.set(false);
                match result {
                    Ok(result) => {
                        app.preferences.replace(result.preferences);
                        if let Some(session) = result.session {
                            app.attach_session(session);
                        } else {
                            app.snapshot.borrow_mut().take();
                            app.startup_error("relay_restore_failed");
                        }
                        app.error(
                            result
                                .error
                                .unwrap_or(if maintenance == Maintenance::Cache {
                                    "storage_transfer_cache_cleared"
                                } else if maintenance == Maintenance::Temporary {
                                    "linux_storage_cleaned"
                                } else {
                                    "relay_settings_applied"
                                }),
                        );
                    }
                    Err(_) => app.startup_error("relay_restore_failed"),
                }
            }
            app.reconfiguring.set(false);
            app.window.set_sensitive(true);
            app.pump_invitations();
            drop(hold);
        });
    }

    pub(super) fn show_preferences(self: &Rc<Self>) {
        if self.saving_preferences.get() {
            self.error("progress_working");
            return;
        }
        if self.window.visible_dialog().is_some() {
            return;
        }
        let dialog = adw::PreferencesDialog::builder()
            .title(text("preferences_title"))
            .build();
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();
        let preferences = self.preferences.borrow().clone();
        let username = adw::EntryRow::builder()
            .title(text("field_username"))
            .text(&preferences.username)
            .build();
        group.add(&username);
        let folder_path = Rc::new(RefCell::new(preferences.receive_directory.clone()));
        let folder = fact(
            "preferences_receive_folder_title",
            &preferences.receive_directory.display().to_string(),
        );
        let choose = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text(text("linux_choose_destination"))
            .valign(gtk::Align::Center)
            .build();
        let reset = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .tooltip_text(text("linux_reset_receive_folder"))
            .valign(gtk::Align::Center)
            .build();
        folder.add_suffix(&reset);
        folder.add_suffix(&choose);
        group.add(&folder);
        let (weak, path, row) = (Rc::downgrade(self), folder_path.clone(), folder.clone());
        choose.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let (path, row) = (path.clone(), row.clone());
            glib::spawn_future_local(async move {
                if let Some(folder) = app.choose_folder().await {
                    row.set_subtitle(&glib::markup_escape_text(&folder.display().to_string()));
                    path.replace(folder);
                }
            });
        });
        let (path, row) = (folder_path.clone(), folder.clone());
        reset.connect_clicked(move |_| {
            let folder = glib::user_special_dir(glib::UserDirectory::Downloads)
                .unwrap_or_else(|| glib::home_dir().join("Downloads"));
            row.set_subtitle(&glib::markup_escape_text(&folder.display().to_string()));
            path.replace(folder);
        });
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
        wrap_choices(&theme);
        let weak = Rc::downgrade(self);
        theme.connect_selected_notify(move |row| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let mut next = app
                .preference_queue
                .borrow()
                .back()
                .cloned()
                .unwrap_or_else(|| app.preferences.borrow().clone());
            next.theme = ["System", "Light", "Dark"][row.selected() as usize].into();
            app.application
                .style_manager()
                .set_color_scheme(match row.selected() {
                    1 => adw::ColorScheme::ForceLight,
                    2 => adw::ColorScheme::ForceDark,
                    _ => adw::ColorScheme::Default,
                });
            app.save_preferences(next);
        });
        group.add(&theme);
        let notifications = adw::SwitchRow::builder()
            .title(text("notifications_local_title"))
            .subtitle(text("notifications_description"))
            .active(preferences.notifications)
            .build();
        group.add(&notifications);
        let save = icon_button("linux_save_preferences", "document-save-symbolic");
        save.add_css_class("suggested-action");
        save.set_margin_top(12);
        group.add(&save);
        let (weak, close) = (Rc::downgrade(self), dialog.clone());
        save.connect_clicked(move |button| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let mut next = app.preferences.borrow().clone();
            next.username = username.text().trim().into();
            next.receive_directory = folder_path.borrow().clone();
            next.theme = ["System", "Light", "Dark"][theme.selected() as usize].into();
            next.notifications = notifications.is_active();
            if next.username.is_empty() {
                app.error("error_invalid_input");
                return;
            }
            let (app, close, button) = (app.clone(), close.clone(), button.clone());
            button.set_sensitive(false);
            glib::spawn_future_local(async move {
                if next.notifications && !super::notifications::available().await {
                    app.error("notifications_unsupported");
                    button.set_sensitive(true);
                    return;
                }
                let path = next.receive_directory.clone();
                let result =
                    gio::spawn_blocking(move || vnidrop_gnome::settings::validate_folder(&path))
                        .await
                        .unwrap_or(Err("error_filesystem"));
                match result {
                    Ok(()) => {
                        app.save_preferences(next);
                        close.close();
                    }
                    Err(key) => app.error(key),
                }
                button.set_sensitive(true);
            });
        });
        page.add(&group);
        let network = adw::PreferencesGroup::builder()
            .title(text("linux_network_policy"))
            .description(text("relay_apply_restart_description"))
            .build();
        let modes = gtk::StringList::new(&[
            &text("relay_mode_automatic"),
            &text("relay_mode_local_only"),
            &text("relay_mode_custom"),
            &text("relay_mode_custom_direct_fallback"),
        ]);
        let mode = adw::ComboRow::builder()
            .title(text("linux_network_policy"))
            .model(&modes)
            .selected(match preferences.network.mode {
                Mode::Automatic => 0,
                Mode::LocalOnly => 1,
                Mode::StrictCustom => 2,
                Mode::CustomWithDirectFallback => 3,
            })
            .build();
        wrap_choices(&mode);
        network.add(&mode);
        let urls = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .top_margin(12)
            .bottom_margin(12)
            .left_margin(12)
            .right_margin(12)
            .height_request(90)
            .build();
        urls.buffer()
            .set_text(&preferences.network.relay_urls.join("\n"));
        urls.update_property(&[gtk::accessible::Property::Label(&text(
            "relay_custom_urls_label",
        ))]);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&urls)
            .min_content_height(90)
            .max_content_height(160)
            .propagate_natural_height(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scroll.add_css_class("card");
        scroll.set_overflow(gtk::Overflow::Hidden);
        scroll.set_margin_top(12);
        scroll.set_margin_bottom(8);
        urls.set_accepts_tab(false);
        network.add(&scroll);
        let help = gtk::Label::builder()
            .label(text("relay_custom_urls_help"))
            .wrap(true)
            .xalign(0.0)
            .build();
        help.add_css_class("dim-label");
        network.add(&help);
        let description = gtk::Label::builder().wrap(true).xalign(0.0).build();
        let update_mode = {
            let urls = urls.clone();
            let scroll = scroll.clone();
            let help = help.clone();
            let description = description.clone();
            move |mode: &adw::ComboRow| {
                urls.set_sensitive(mode.selected() >= 2);
                scroll.set_visible(mode.selected() >= 2);
                help.set_visible(mode.selected() >= 2);
                description.set_label(&text(
                    [
                        "relay_mode_automatic_description",
                        "relay_mode_local_only_description",
                        "relay_strict_warning",
                        "relay_mode_custom_direct_fallback_description",
                    ][mode.selected() as usize],
                ));
            }
        };
        update_mode(&mode);
        mode.connect_selected_notify(update_mode);
        description.set_margin_top(12);
        network.add(&description);
        let apply = icon_button("relay_apply", "network-transmit-receive-symbolic");
        apply.set_margin_top(12);
        network.add(&apply);
        let (weak, close) = (Rc::downgrade(self), dialog.clone());
        apply.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let mut next = app.preferences.borrow().clone();
            let buffer = urls.buffer();
            let input = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            match vnidrop_gnome::settings::network(
                [
                    Mode::Automatic,
                    Mode::LocalOnly,
                    Mode::StrictCustom,
                    Mode::CustomWithDirectFallback,
                ][mode.selected() as usize],
                &input,
                &next.network.relay_urls,
            ) {
                Ok(config) => {
                    next.network = config;
                    close.close();
                    app.reconfigure(next, Maintenance::None);
                }
                Err(key) => app.error(key),
            }
        });
        page.add(&network);
        let storage = adw::PreferencesGroup::builder()
            .title(text("storage_title"))
            .description(text("storage_footer"))
            .build();
        let usage = gtk::Box::new(gtk::Orientation::Vertical, 0);
        storage.add(&usage);
        let refresh = icon_button("storage_refresh", "view-refresh-symbolic");
        refresh.set_margin_top(12);
        storage.add(&refresh);
        let (weak, rows) = (Rc::downgrade(self), usage.clone());
        refresh.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.storage_usage(&rows);
            }
        });
        self.storage_usage(&usage);
        let clear = icon_button("storage_clear_transfer_cache", "edit-clear-all-symbolic");
        clear.set_margin_top(12);
        storage.add(&clear);
        let (weak, close) = (Rc::downgrade(self), dialog.clone());
        clear.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            close.force_close();
            glib::spawn_future_local(async move {
                if dialogs::confirm(
                    &app.window,
                    "storage_clear_transfer_cache",
                    "storage_clear_transfer_cache_description",
                    "storage_clear_transfer_cache",
                )
                .await
                {
                    let next = app.preferences.borrow().clone();
                    app.reconfigure(next, Maintenance::Cache);
                }
            });
        });
        let cleanup = icon_button("storage_free_up_space", "edit-clear-symbolic");
        cleanup.set_margin_top(12);
        storage.add(&cleanup);
        let (weak, close) = (Rc::downgrade(self), dialog.clone());
        cleanup.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            close.force_close();
            glib::spawn_future_local(async move {
                if dialogs::confirm(
                    &app.window,
                    "storage_free_up_space",
                    "linux_temporary_cleanup_body",
                    "storage_free_up_space",
                )
                .await
                {
                    let next = app.preferences.borrow().clone();
                    app.reconfigure(next, Maintenance::Temporary);
                }
            });
        });
        page.add(&storage);
        dialog.add(&page);
        dialog.present(Some(&self.window));
    }

    fn storage_usage(self: &Rc<Self>, rows: &gtk::Box) {
        if self.reconfiguring.get() || self.closing.get() || self.session.borrow().is_none() {
            return;
        }
        self.settings_reads.set(self.settings_reads.get() + 1);
        let rows = rows.clone();
        let directory = self.preferences.borrow().receive_directory.clone();
        self.dispatch(
            move |session| {
                let (usage, artifacts) = session
                    .call(|core| Ok((core.storage_usage()?, core.list_received_artifacts()?)))?;
                let received = artifacts
                    .iter()
                    .filter_map(|artifact| std::fs::symlink_metadata(&artifact.locator).ok())
                    .filter(|metadata| metadata.is_file())
                    .map(|metadata| metadata.len())
                    .sum::<u64>();
                let temporary = vnidrop_gnome::storage::temporary_usage(&directory);
                Ok((usage, received, temporary))
            },
            move |app, result| {
                app.settings_reads.set(app.settings_reads.get() - 1);
                while let Some(child) = rows.first_child() {
                    rows.remove(&child);
                }
                match result {
                    Ok((
                        CoreStorageUsage {
                            blob_store_bytes,
                            database_bytes,
                            logs_bytes,
                            previews_bytes,
                            other_core_bytes,
                        },
                        received,
                        temporary,
                    )) => {
                        let group = adw::PreferencesGroup::new();
                        for (key, bytes) in [
                            ("storage_transfer_data", blob_store_bytes),
                            ("storage_received_files", received),
                            (
                                "storage_app_data",
                                database_bytes + logs_bytes + previews_bytes + other_core_bytes,
                            ),
                            (
                                "storage_total",
                                blob_store_bytes
                                    + database_bytes
                                    + logs_bytes
                                    + previews_bytes
                                    + other_core_bytes
                                    + received
                                    + temporary.unwrap_or(0),
                            ),
                        ] {
                            group.add(&fact(key, &glib::format_size(bytes)));
                        }
                        group.add(&fact(
                            "storage_temporary",
                            &temporary
                                .map(|bytes| glib::format_size(bytes).to_string())
                                .unwrap_or_else(|_| text("storage_unavailable")),
                        ));
                        rows.append(&group);
                    }
                    Err(key) => app.error(key),
                }
            },
        );
    }
}

fn wrap_choices(row: &adw::ComboRow) {
    row.set_use_subtitle(true);
    row.set_subtitle_lines(0);
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let item = object.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .max_width_chars(28)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, object| {
        let item = object.downcast_ref::<gtk::ListItem>().unwrap();
        let value = item.item().and_downcast::<gtk::StringObject>().unwrap();
        item.child()
            .and_downcast::<gtk::Label>()
            .unwrap()
            .set_label(&value.string());
    });
    row.set_list_factory(Some(&factory));
}
