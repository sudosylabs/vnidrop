use super::{i18n::text, widgets::icon_button, App};
use adw::prelude::*;
use gtk::glib;
use std::{cell::Cell, rc::Rc};

impl App {
    pub(super) fn show_about(self: &Rc<Self>) {
        if self.window.visible_dialog().is_some() {
            return;
        }
        let dialog = adw::PreferencesDialog::builder()
            .title(text("about_title"))
            .build();
        let page = adw::PreferencesPage::new();
        let intro = adw::PreferencesGroup::new();
        let header = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let icon = gtk::Image::from_icon_name("com.vnidrop.VniDrop");
        icon.set_pixel_size(80);
        header.append(&icon);
        let name = gtk::Label::new(Some("VniDrop"));
        name.add_css_class("title-1");
        header.append(&name);
        for key in ["about_tagline", "about_description"] {
            let label = gtk::Label::builder()
                .label(text(key))
                .wrap(true)
                .justify(gtk::Justification::Center)
                .build();
            header.append(&label);
        }
        intro.add(&header);
        page.add(&intro);
        for (title, points) in [
            (
                "about_is_title",
                vec![
                    ("mail-send-symbolic", "about_is_direct"),
                    ("avatar-default-symbolic", "about_is_no_account"),
                    ("security-high-symbolic", "about_is_in_control"),
                    ("channel-secure-symbolic", "about_is_encrypted"),
                    ("text-x-script-symbolic", "about_is_open"),
                ],
            ),
            (
                "about_isnt_title",
                vec![
                    ("network-server-symbolic", "about_isnt_cloud"),
                    ("view-refresh-symbolic", "about_isnt_sync"),
                    ("system-users-symbolic", "about_isnt_public"),
                ],
            ),
            (
                "about_privacy_title",
                vec![
                    ("qr-code-symbolic", "about_invitation_privacy"),
                    ("action-unavailable-symbolic", "about_privacy_deny"),
                    ("network-transmit-receive-symbolic", "about_privacy_relay"),
                    ("drive-harddisk-symbolic", "about_privacy_local"),
                ],
            ),
        ] {
            let group = adw::PreferencesGroup::builder()
                .title(glib::markup_escape_text(&text(title)))
                .build();
            for (icon, key) in points {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&text(key)))
                    .title_lines(0)
                    .build();
                row.add_prefix(&gtk::Image::from_icon_name(icon));
                group.add(&row);
            }
            page.add(&group);
        }
        let info = adw::PreferencesGroup::new();
        let version = include_str!("../../../version.properties")
            .lines()
            .find_map(|line| line.strip_prefix("PRODUCT_VERSION="))
            .unwrap_or(env!("CARGO_PKG_VERSION"));
        let os = glib::os_info("PRETTY_NAME").unwrap_or_else(|| "Linux".into());
        for (key, value) in [
            ("version_title", version),
            ("device_model_title", std::env::consts::ARCH),
            ("os_version_title", os.as_str()),
            ("about_license_label", "Apache 2.0"),
        ] {
            info.add(
                &adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&text(key)))
                    .subtitle(glib::markup_escape_text(value))
                    .build(),
            );
        }
        let privacy = include_str!("../../../app.properties")
            .lines()
            .find_map(|line| line.strip_prefix("PRIVACY_POLICY_URL="))
            .expect("privacy policy URL");
        info.add(&gtk::LinkButton::with_label(
            privacy,
            &text("about_privacy_policy_label"),
        ));
        let license = icon_button("about_license_label", "text-x-generic-symbolic");
        let weak = Rc::downgrade(self);
        license.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                adw::AboutDialog::builder()
                    .application_name("VniDrop")
                    .application_icon("com.vnidrop.VniDrop")
                    .version(version)
                    .developer_name("Sudosy Labs")
                    .license_type(gtk::License::Apache20)
                    .website("https://github.com/vnidrop/vnidrop")
                    .build()
                    .present(Some(&app.window));
            }
        });
        info.add(&license);
        page.add(&info);
        let actions = adw::PreferencesGroup::new();
        let report = icon_button("about_bug_report", "tools-report-bug-symbolic");
        actions.add(&report);
        page.add(&actions);
        let reporting = Rc::new(Cell::new(false));
        let (closing, flag) = (dialog.clone(), reporting.clone());
        report.connect_clicked(move |_| {
            flag.set(true);
            closing.close();
        });
        let weak = Rc::downgrade(self);
        dialog.connect_closed(move |_| {
            if reporting.get() {
                let weak = weak.clone();
                glib::idle_add_local_once(move || {
                    if let Some(app) = weak.upgrade() {
                        app.show_bug_report();
                    }
                });
            }
        });
        dialog.add(&page);
        dialog.present(Some(&self.window));
    }
}
