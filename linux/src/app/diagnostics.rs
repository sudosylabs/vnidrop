use super::{dialogs, i18n::text, widgets::icon_button, App};
use adw::prelude::*;
use gtk::{gio, glib};
use std::rc::Rc;
use vnidrop_gnome::diagnostics::{self, Configuration, Draft};

impl App {
    pub(super) fn show_bug_report(self: &Rc<Self>) {
        self.present_bug_report(Configuration::configured());
    }

    pub(super) fn present_bug_report(
        self: &Rc<Self>,
        configuration: vnidrop_gnome::error::Result<Configuration>,
    ) {
        if self.saving_preferences.get() {
            self.error("progress_working");
            return;
        }
        if self.window.visible_dialog().is_some() {
            return;
        }
        let (dialog, content) = dialogs::scrollable_content("about_bug_report");
        let intro = gtk::Label::builder()
            .label(text("bug_report_description"))
            .wrap(true)
            .xalign(0.0)
            .build();
        content.append(&intro);
        let device = adw::PreferencesGroup::builder()
            .title(text("bug_report_device_section"))
            .build();
        device.add(&dialogs::fact(
            "field_username",
            &self.preferences.borrow().username,
        ));
        device.add(&dialogs::fact(
            "linux_about",
            &format!(
                "VniDrop {} · Linux · {}",
                diagnostics::version(),
                std::env::consts::ARCH
            ),
        ));
        content.append(&device);
        let mut fields = Vec::new();
        for (index, key) in [
            "bug_report_what_label",
            "bug_report_expected_label",
            "bug_report_steps_label",
        ]
        .into_iter()
        .enumerate()
        {
            let label = gtk::Label::builder()
                .label(text(key))
                .xalign(0.0)
                .wrap(true)
                .build();
            label.add_css_class("heading");
            content.append(&label);
            let editor = gtk::TextView::builder()
                .wrap_mode(gtk::WrapMode::WordChar)
                .top_margin(8)
                .bottom_margin(8)
                .left_margin(8)
                .right_margin(8)
                .build();
            editor.set_accepts_tab(false);
            let draft = &self.report.borrow().draft;
            editor.buffer().set_text(match index {
                0 => &draft.what,
                1 => &draft.expected,
                _ => &draft.steps,
            });
            let weak = Rc::downgrade(self);
            editor.buffer().connect_changed(move |buffer| {
                if let Some(app) = weak.upgrade() {
                    let value = buffer
                        .text(&buffer.start_iter(), &buffer.end_iter(), false)
                        .to_string();
                    let draft = &mut app.report.borrow_mut().draft;
                    match index {
                        0 => draft.what = value,
                        1 => draft.expected = value,
                        _ => draft.steps = value,
                    }
                }
            });
            editor.update_property(&[gtk::accessible::Property::Label(&text(key))]);
            let scroll = gtk::ScrolledWindow::builder()
                .child(&editor)
                .min_content_height(90)
                .max_content_height(160)
                .propagate_natural_height(true)
                .hscrollbar_policy(gtk::PolicyType::Never)
                .build();
            scroll.add_css_class("card");
            scroll.set_overflow(gtk::Overflow::Hidden);
            content.append(&scroll);
            fields.push(editor);
        }
        let group = adw::PreferencesGroup::new();
        let contact = adw::EntryRow::builder()
            .title(text("bug_report_contact_label"))
            .text(&self.report.borrow().draft.contact)
            .build();
        let weak = Rc::downgrade(self);
        contact.connect_changed(move |entry| {
            if let Some(app) = weak.upgrade() {
                app.report.borrow_mut().draft.contact = entry.text().to_string();
            }
        });
        group.add(&contact);
        let logs = adw::SwitchRow::builder()
            .title(text("bug_report_include_logs"))
            .subtitle(text("linux_report_logs_description"))
            .active(self.report.borrow().draft.include_logs)
            .build();
        let weak = Rc::downgrade(self);
        logs.connect_active_notify(move |row| {
            if let Some(app) = weak.upgrade() {
                app.report.borrow_mut().draft.include_logs = row.is_active();
            }
        });
        group.add(&logs);
        content.append(&group);
        let error = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .visible(false)
            .build();
        error.add_css_class("error");
        error.set_focusable(true);
        content.append(&error);
        let submit = icon_button("bug_report_submit", "mail-send-symbolic");
        submit.add_css_class("suggested-action");
        content.append(&submit);
        if configuration.is_err() {
            error.set_label(&text("linux_diagnostics_unconfigured"));
            error.set_visible(true);
            submit.set_sensitive(false);
        }
        let (weak, close) = (Rc::downgrade(self), dialog.clone());
        submit.connect_clicked(move |button| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let values: Vec<_> = fields
                .iter()
                .map(|view| {
                    let b = view.buffer();
                    b.text(&b.start_iter(), &b.end_iter(), false).to_string()
                })
                .collect();
            let draft = Draft {
                what: values[0].clone(),
                expected: values[1].clone(),
                steps: values[2].clone(),
                contact: contact.text().to_string(),
                include_logs: logs.is_active(),
            };
            let mut prefs = app.preferences.borrow().clone();
            if prefs.diagnostics_install_id.is_empty() {
                prefs.diagnostics_install_id = uuid::Uuid::new_v4().to_string();
            }
            let events = app
                .snapshot
                .borrow()
                .as_ref()
                .map(|s| s.events.clone())
                .unwrap_or_default();
            app.report.borrow_mut().draft = draft;
            let report = match app.report.borrow_mut().prepare(
                &prefs.diagnostics_install_id,
                &prefs.username,
                &format!("{:?}", prefs.network.mode),
                &events,
            ) {
                Ok(report) => report,
                Err(key) => {
                    error.set_label(&text(key));
                    error.set_visible(true);
                    return;
                }
            };
            content.set_sensitive(false);
            super::widgets::set_icon_button_label(button, "bug_report_submitting");
            close.set_can_close(false);
            error.set_visible(false);
            let (button, close, error) = (button.clone(), close.clone(), error.clone());
            let profile = app.profile.clone();
            let content = content.clone();
            app.busy.set(app.busy.get() + 1);
            let hold = app.application.hold();
            let configuration = configuration.clone();
            glib::spawn_future_local(async move {
                let saved = prefs.clone();
                let (persisted, result) = gio::spawn_blocking(move || match prefs.save(&profile) {
                    Ok(()) => (
                        true,
                        configuration.and_then(|configuration| configuration.send(&report)),
                    ),
                    Err(key) => (false, Err(key)),
                })
                .await
                .unwrap_or((false, Err("bug_report_submit_failed")));
                app.busy.set(app.busy.get() - 1);
                if persisted {
                    app.preferences.replace(saved);
                }
                if app.closing.get() {
                    drop(hold);
                    return;
                }
                close.set_can_close(true);
                content.set_sensitive(true);
                super::widgets::set_icon_button_label(&button, "bug_report_submit");
                match result {
                    Ok(id) => {
                        app.report.borrow_mut().clear();
                        while let Some(child) = content.first_child() {
                            content.remove(&child);
                        }
                        let icon = gtk::Image::from_icon_name("emblem-ok-symbolic");
                        icon.set_pixel_size(64);
                        icon.add_css_class("success");
                        content.append(&icon);
                        let heading = gtk::Label::builder()
                            .label(text("bug_report_submitted"))
                            .wrap(true)
                            .build();
                        heading.add_css_class("title-1");
                        content.append(&heading);
                        let receipt = gtk::Label::builder()
                            .label(super::i18n::format("linux_report_receipt", &[("id", &id)]))
                            .wrap(true)
                            .selectable(true)
                            .build();
                        content.append(&receipt);
                        let done = icon_button("button_close", "window-close-symbolic");
                        let dialog = close.clone();
                        done.connect_clicked(move |_| {
                            dialog.close();
                        });
                        content.append(&done);
                        done.grab_focus();
                    }
                    Err(key) => {
                        error.set_label(&text(key));
                        error.set_visible(true);
                        error.grab_focus();
                    }
                }
                drop(hold);
            });
        });
        dialog.present(Some(&self.window));
    }
}
