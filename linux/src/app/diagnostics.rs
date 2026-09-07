use super::{dialogs, i18n::text, widgets::icon_button, App};
use adw::prelude::*;
use gtk::{gio, glib};
use std::{cell::RefCell, rc::Rc};
use vnidrop_gnome::diagnostics::{self, Configuration, Draft};

impl App {
    pub(super) fn show_bug_report(self: &Rc<Self>) {
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
        for key in [
            "bug_report_what_label",
            "bug_report_expected_label",
            "bug_report_steps_label",
        ] {
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
            .build();
        group.add(&contact);
        let logs = adw::SwitchRow::builder()
            .title(text("bug_report_include_logs"))
            .subtitle(text("linux_report_logs_description"))
            .active(true)
            .build();
        group.add(&logs);
        content.append(&group);
        let error = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .visible(false)
            .build();
        error.add_css_class("error");
        content.append(&error);
        let submit = icon_button("bug_report_submit", "mail-send-symbolic");
        submit.add_css_class("suggested-action");
        content.append(&submit);
        if Configuration::configured().is_err() {
            error.set_label(&text("linux_diagnostics_unconfigured"));
            error.set_visible(true);
            submit.set_sensitive(false);
        }
        let cached = Rc::new(RefCell::new(None::<(String, serde_json::Value)>));
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
            let report = match diagnostics::assemble(
                &draft,
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
            let fingerprint =
                serde_json::to_string(&(values, contact.text().as_str(), logs.is_active()))
                    .unwrap();
            let report = if let Some((previous, report)) = cached
                .borrow()
                .as_ref()
                .filter(|(previous, _)| previous == &fingerprint)
            {
                let _ = previous;
                report.clone()
            } else {
                report
            };
            cached.replace(Some((fingerprint, report.clone())));
            content.set_sensitive(false);
            super::widgets::set_icon_button_label(button, "bug_report_submitting");
            close.set_can_close(false);
            error.set_visible(false);
            let (button, close, error) = (button.clone(), close.clone(), error.clone());
            let profile = app.profile.clone();
            let content = content.clone();
            app.busy.set(app.busy.get() + 1);
            let hold = app.application.hold();
            glib::spawn_future_local(async move {
                let saved = prefs.clone();
                let (persisted, result) = gio::spawn_blocking(move || match prefs.save(&profile) {
                    Ok(()) => (
                        true,
                        Configuration::configured()
                            .and_then(|configuration| configuration.send(&report)),
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
                    Ok(()) => {
                        close.close();
                        app.error("bug_report_submitted");
                    }
                    Err(key) => {
                        error.set_label(&text(key));
                        error.set_visible(true);
                    }
                }
                drop(hold);
            });
        });
        dialog.present(Some(&self.window));
    }
}
