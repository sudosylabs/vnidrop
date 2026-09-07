use super::{
    dialogs,
    i18n::{format, text},
    widgets::icon_button,
    App,
};
use adw::prelude::*;
use gtk::glib;
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    rc::Rc,
    time::Duration,
};
use vnidrop_gnome::devices::{Device, DeviceAction, DeviceState};

pub(super) struct DevicesView {
    pub root: gtk::Stack,
    pub split: Rc<super::device_layout::DeviceLayout>,
    rows: gtk::ListBox,
    pub details: gtk::Box,
    detail_page: adw::NavigationPage,
    manage: gtk::Button,
    management: RefCell<Option<(adw::Dialog, Rc<DeviceEditor>)>>,
    rendered: RefCell<Option<(Vec<Device>, BTreeSet<String>)>>,
    pub selected: RefCell<Option<String>>,
    pub editor: RefCell<Option<Rc<DeviceEditor>>>,
    pub transfers: RefCell<Option<Rc<super::device_transfers::DeviceTransfers>>>,
    drafts: RefCell<HashMap<String, String>>,
    remote_names: RefCell<HashMap<String, String>>,
}

pub(super) struct DeviceEditor {
    pub target: Device,
    pub primary: RefCell<Option<gtk::Button>>,
    error: gtk::Label,
    controls: gtk::Box,
}

impl App {
    pub(super) fn schedule_device_expiry(self: &Rc<Self>, deadline: Option<i64>) {
        if self.device_expiry.borrow().as_ref().map(|(time, _)| *time) == deadline {
            return;
        }
        if let Some((_, timer)) = self.device_expiry.borrow_mut().take() {
            timer.remove();
        }
        if let Some(deadline) = deadline {
            let weak = Rc::downgrade(self);
            let timer = glib::timeout_add_local_once(
                Duration::from_millis((deadline - glib::real_time() / 1000).max(1) as u64),
                move || {
                    if let Some(app) = weak.upgrade() {
                        app.device_expiry.borrow_mut().take();
                        if !app.closing.get() {
                            app.refresh();
                        }
                    }
                },
            );
            self.device_expiry.replace(Some((deadline, timer)));
        }
    }

    pub(super) fn show_devices(self: &Rc<Self>) {
        if self.window.visible_dialog().is_some() {
            return;
        }
        if self.devices.borrow().is_none() {
            let view = DevicesView::new(self);
            self.object::<gtk::Stack>("root")
                .add_named(&view.root, Some("devices"));
            self.devices.replace(Some(view));
        }
        self.clear_review_highlight();
        self.showing_devices.set(true);
        self.render();
    }

    fn device_action(self: &Rc<Self>, target: Device, action: DeviceAction) {
        self.clear_review_highlight();
        if !self.device_busy.borrow_mut().insert(target.peer.clone()) {
            return;
        }
        let peer = target.peer.clone();
        let view = self.devices.borrow().clone();
        if let Some(view) = view {
            view.render(self);
        }
        self.dispatch(
            move |session| {
                session.call(|core| {
                    Ok(vnidrop_gnome::devices::execute(
                        core,
                        &target,
                        action,
                        glib::real_time() / 1000,
                    ))
                })?
            },
            move |app, result| {
                app.device_busy.borrow_mut().remove(&peer);
                let view = app.devices.borrow().clone();
                if let Some(view) = view {
                    let editor = view
                        .management
                        .borrow()
                        .as_ref()
                        .map(|(_, editor)| editor.clone())
                        .or_else(|| view.editor.borrow().clone());
                    if let Some(editor) = editor.filter(|editor| editor.target.peer == peer) {
                        match result {
                            Ok(()) => {
                                view.drafts.borrow_mut().remove(&peer);
                                let dialog = view
                                    .management
                                    .borrow()
                                    .as_ref()
                                    .map(|(dialog, _)| dialog.clone());
                                if let Some(dialog) = dialog {
                                    dialog.close();
                                }
                                view.editor.borrow_mut().take();
                            }
                            Err(key) => {
                                editor.error.set_label(&text(key));
                                editor.error.set_visible(true);
                            }
                        }
                    } else if let Err(key) = result {
                        app.error(key);
                    }
                    view.render(&app);
                } else if let Err(key) = result {
                    app.error(key);
                }
                app.refresh();
            },
        );
    }
}
impl DevicesView {
    fn new(app: &Rc<App>) -> Rc<Self> {
        let rows = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        rows.add_css_class("navigation-sidebar");
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&rows)
            .build();
        let header = adw::HeaderBar::new();
        let transfers = icon_button("linux_transfers", "go-previous-symbolic");
        transfers.set_action_name(Some("app.transfers"));
        header.pack_start(&transfers);
        let toolbar = adw::ToolbarView::builder().content(&scroll).build();
        toolbar.add_top_bar(&header);
        let sidebar_header = header;
        let sidebar = adw::NavigationPage::builder()
            .title(text("nav_saved_devices"))
            .child(&toolbar)
            .build();
        let details = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        let clamp = adw::Clamp::builder()
            .maximum_size(620)
            .child(&details)
            .build();
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&clamp)
            .build();
        let header = adw::HeaderBar::new();
        let manage = gtk::Button::builder()
            .icon_name("emblem-system-symbolic")
            .tooltip_text(text("linux_manage_device"))
            .visible(false)
            .build();
        header.pack_end(&manage);
        let toolbar = adw::ToolbarView::builder().content(&scroll).build();
        toolbar.add_top_bar(&header);
        let detail_page = adw::NavigationPage::builder()
            .title(text("nav_saved_devices"))
            .child(&toolbar)
            .build();
        let split = super::device_layout::DeviceLayout::new(
            sidebar,
            detail_page.clone(),
            sidebar_header,
            &header,
        );
        split.set_collapsed(app.split.is_collapsed());
        let weak_split = Rc::downgrade(&split);
        app.split.connect_collapsed_notify(move |source| {
            if let Some(split) = weak_split.upgrade() {
                split.set_collapsed(source.is_collapsed());
            }
        });
        let empty = adw::StatusPage::builder()
            .title(text("saved_devices_empty_title"))
            .description(text("saved_devices_empty"))
            .icon_name("computer-symbolic")
            .vexpand(true)
            .build();
        empty.add_css_class("compact");
        let transfers = icon_button("linux_transfers", "go-previous-symbolic");
        transfers.set_action_name(Some("app.transfers"));
        transfers.add_css_class("pill");
        transfers.set_halign(gtk::Align::Center);
        empty.set_child(Some(&transfers));
        let toolbar = adw::ToolbarView::builder().content(&empty).build();
        let header = adw::HeaderBar::builder()
            .title_widget(&adw::WindowTitle::new(&text("nav_saved_devices"), ""))
            .build();
        toolbar.add_top_bar(&header);
        let root = gtk::Stack::new();
        root.add_named(&toolbar, Some("empty"));
        root.add_named(&split.root, Some("list"));
        let view = Rc::new(Self {
            root,
            split,
            rows,
            details,
            detail_page,
            manage,
            management: RefCell::new(None),
            rendered: RefCell::new(None),
            selected: RefCell::new(None),
            editor: RefCell::new(None),
            transfers: RefCell::new(None),
            drafts: RefCell::new(HashMap::new()),
            remote_names: RefCell::new(HashMap::new()),
        });
        let (weak, weak_app) = (Rc::downgrade(&view), Rc::downgrade(app));
        view.manage.connect_clicked(move |_| {
            if let (Some(view), Some(app)) = (weak.upgrade(), weak_app.upgrade()) {
                view.show_management(&app);
            }
        });
        view
    }

    pub(super) fn show_management(self: &Rc<Self>, app: &Rc<App>) {
        if app.window.visible_dialog().is_some() {
            return;
        }
        let Some(target) = self
            .editor
            .borrow()
            .as_ref()
            .map(|editor| editor.target.clone())
        else {
            return;
        };
        if !matches!(
            target.state,
            DeviceState::Saved { .. } | DeviceState::Blocked
        ) {
            return;
        }
        let (dialog, rows, _) = dialogs::content("linux_manage_device");
        let editor = self.build_editor(app, target, &rows, true);
        let weak = Rc::downgrade(self);
        dialog.connect_closed(move |_| {
            if let Some(view) = weak.upgrade() {
                view.management.borrow_mut().take();
            }
        });
        self.management.replace(Some((dialog.clone(), editor)));
        dialog.present(Some(&app.window));
    }

    pub fn render(self: &Rc<Self>, app: &Rc<App>) {
        let mut devices = app
            .snapshot
            .borrow()
            .as_ref()
            .map(|s| s.devices.list(glib::real_time() / 1000))
            .unwrap_or_default();
        // Eligibility is consumed during pairing; keep the authenticated name while consent is pending.
        for device in &mut devices {
            if let Some(name) = &device.remote_name {
                self.remote_names
                    .borrow_mut()
                    .insert(device.peer.clone(), name.clone());
            }
            if matches!(
                device.state,
                DeviceState::Incoming { .. } | DeviceState::Outgoing { .. }
            ) && device.name.is_none()
            {
                device.remote_name = self.remote_names.borrow().get(&device.peer).cloned();
                device.name.clone_from(&device.remote_name);
            }
        }
        self.drafts.borrow_mut().retain(|peer, _| {
            devices.iter().any(|device| {
                device.peer == *peer && matches!(device.state, DeviceState::Saved { .. })
            })
        });
        self.remote_names
            .borrow_mut()
            .retain(|peer, _| devices.iter().any(|device| device.peer == *peer));
        let busy = app.device_busy.borrow().clone();
        self.root
            .set_visible_child_name(if devices.is_empty() { "empty" } else { "list" });
        let management = self.management.borrow().clone();
        if let Some((dialog, editor)) = management {
            if devices.iter().any(|device| device == &editor.target) {
                editor
                    .controls
                    .set_sensitive(!busy.contains(&editor.target.peer));
            } else {
                dialog.close();
            }
        }
        if self
            .selected
            .borrow()
            .as_ref()
            .is_some_and(|peer| !devices.iter().any(|d| d.peer == *peer))
        {
            self.selected.replace(None);
            self.split.set_show_content(false);
        }
        if self.rendered.borrow().as_ref() != Some(&(devices.clone(), busy.clone())) {
            let list_had_focus = gtk::prelude::RootExt::focus(&app.window)
                .is_some_and(|focus| focus.is_ancestor(&self.rows));
            self.rendered.replace(Some((devices.clone(), busy.clone())));
            while let Some(child) = self.rows.first_child() {
                self.rows.remove(&child);
            }
            for device in &devices {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&device_name(device)))
                    .subtitle(text(device.status_key()))
                    .title_lines(2)
                    .subtitle_lines(2)
                    .activatable(true)
                    .build();
                row.add_prefix(&gtk::Image::from_icon_name(
                    if matches!(device.state, DeviceState::Blocked) {
                        "action-unavailable-symbolic"
                    } else {
                        "computer-symbolic"
                    },
                ));
                row.set_sensitive(!busy.contains(&device.peer));
                let (weak, weak_app, target) =
                    (Rc::downgrade(self), Rc::downgrade(app), device.clone());
                row.connect_activated(move |_| {
                    if let (Some(view), Some(app)) = (weak.upgrade(), weak_app.upgrade()) {
                        view.show_device(&app, target.clone());
                    }
                });
                self.rows.append(&row);
                if self.selected.borrow().as_ref() == Some(&device.peer) {
                    self.rows.select_row(Some(&row));
                    if list_had_focus {
                        row.grab_focus();
                    }
                }
            }
        }
        let selected = self.selected.borrow().clone();
        let target = devices
            .into_iter()
            .find(|d| Some(&d.peer) == selected.as_ref());
        let existing = self.editor.borrow().clone();
        if existing.is_some() && existing.as_ref().map(|editor| &editor.target) == target.as_ref() {
            if let Some(transfers) = self.transfers.borrow().as_ref() {
                transfers.render(app);
            }
            if let Some(editor) = existing {
                editor
                    .controls
                    .set_sensitive(!busy.contains(&editor.target.peer));
            }
            return;
        }
        if gtk::prelude::RootExt::focus(&app.window)
            .is_some_and(|focus| focus.is_ancestor(&self.details))
        {
            self.rows.grab_focus();
        }
        self.editor.borrow_mut().take();
        self.transfers.borrow_mut().take();
        while let Some(child) = self.details.first_child() {
            self.details.remove(&child);
        }
        if let Some(target) = target {
            self.manage.set_visible(matches!(
                target.state,
                DeviceState::Saved { .. } | DeviceState::Blocked
            ));
            self.editor.replace(Some(self.build_editor(
                app,
                target.clone(),
                &self.details,
                false,
            )));
            let transfers = super::device_transfers::DeviceTransfers::new(target.peer.clone());
            self.details.append(&transfers.root);
            transfers.render(app);
            self.transfers.replace(Some(transfers));
        } else {
            self.manage.set_visible(false);
            self.detail_page.set_title(&text("nav_saved_devices"));
            let placeholder = adw::StatusPage::builder()
                .title(text("linux_select_device"))
                .icon_name("computer-symbolic")
                .vexpand(true)
                .build();
            placeholder.add_css_class("compact");
            self.details.append(&placeholder);
        }
    }

    pub(super) fn show_device(self: &Rc<Self>, app: &Rc<App>, target: Device) {
        app.clear_review_highlight();
        self.selected.replace(Some(target.peer.clone()));
        self.render(app);
        self.split.set_show_content(true);
        let index = self
            .rendered
            .borrow()
            .as_ref()
            .and_then(|(devices, _)| devices.iter().position(|d| d.peer == target.peer));
        if let Some(index) = index {
            self.rows
                .select_row(self.rows.row_at_index(index as i32).as_ref());
        }
    }

    fn build_editor(
        self: &Rc<Self>,
        app: &Rc<App>,
        target: Device,
        rows: &gtk::Box,
        management: bool,
    ) -> Rc<DeviceEditor> {
        if !management {
            self.detail_page.set_title(&device_name(&target));
        }
        let title = gtk::Label::builder()
            .label(device_name(&target))
            .xalign(0.0)
            .wrap(true)
            .build();
        title.add_css_class("title-1");
        rows.append(&title);
        let description = match target.state {
            DeviceState::Eligible { .. } => {
                format("pairing_request_body", &[("device", &device_name(&target))])
            }
            DeviceState::Incoming { .. } => text("pairing_allow_body"),
            DeviceState::Blocked => text("linux_unblock_description"),
            _ => text(target.status_key()),
        };
        rows.append(
            &gtk::Label::builder()
                .label(description)
                .wrap(true)
                .xalign(0.0)
                .build(),
        );
        let identity = adw::ExpanderRow::builder()
            .title(text("linux_device_identity"))
            .build();
        if let Some(name) = &target.remote_name {
            identity.add_row(&dialogs::fact("device_name_title", name));
        }
        identity.add_row(&dialogs::fact("linux_device_identity", &target.peer));
        let group = adw::PreferencesGroup::new();
        group.add(&identity);
        rows.append(&group);
        let error = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .visible(false)
            .build();
        error.add_css_class("error");
        rows.append(&error);
        let controls = gtk::Box::new(gtk::Orientation::Vertical, 12);
        controls.add_css_class("review-target");
        controls.add_css_class("device-decision");
        rows.append(&controls);
        let editor = Rc::new(DeviceEditor {
            primary: RefCell::new(None),
            target: target.clone(),
            error,
            controls,
        });
        let mut actions = Vec::new();
        match target.state {
            DeviceState::Eligible { .. } => {
                actions.push((
                    "pairing_accept",
                    "emblem-ok-symbolic",
                    DeviceAction::Remember,
                ));
                actions.push((
                    "pairing_decline",
                    "window-close-symbolic",
                    DeviceAction::Decline,
                ));
            }
            DeviceState::Incoming { .. } => {
                actions.push((
                    "pairing_allow_confirm",
                    "emblem-ok-symbolic",
                    DeviceAction::Accept,
                ));
                actions.push((
                    "saved_devices_decline_action",
                    "window-close-symbolic",
                    DeviceAction::Decline,
                ));
            }
            DeviceState::Outgoing { .. } => {}
            DeviceState::Blocked if management => actions.push((
                "linux_unblock_device",
                "changes-allow-symbolic",
                DeviceAction::Unblock,
            )),
            DeviceState::Saved { .. } if management => {
                let label = adw::EntryRow::builder()
                    .title(text("saved_devices_label_title"))
                    .text(
                        self.drafts
                            .borrow()
                            .get(&target.peer)
                            .cloned()
                            .or_else(|| target.label.clone())
                            .as_deref()
                            .unwrap_or(""),
                    )
                    .build();
                let weak = Rc::downgrade(self);
                let peer = target.peer.clone();
                label.connect_changed(move |entry| {
                    if let Some(view) = weak.upgrade() {
                        view.drafts
                            .borrow_mut()
                            .insert(peer.clone(), entry.text().to_string());
                    }
                });
                let group = adw::PreferencesGroup::new();
                group.add(&label);
                editor.controls.append(&group);
                let save = icon_button("saved_devices_label_save", "document-save-symbolic");
                let (weak_app, target) = (Rc::downgrade(app), target.clone());
                save.connect_clicked(move |_| {
                    if let Some(app) = weak_app.upgrade() {
                        app.device_action(
                            target.clone(),
                            DeviceAction::Label(label.text().to_string()),
                        );
                    }
                });
                save.add_css_class("suggested-action");
                editor.controls.append(&save);
                actions.push((
                    "saved_devices_label_clear",
                    "edit-clear-symbolic",
                    DeviceAction::Label(String::new()),
                ));
                actions.push((
                    "saved_devices_forget_action",
                    "edit-delete-symbolic",
                    DeviceAction::Forget,
                ));
                actions.push((
                    "saved_devices_block_action",
                    "action-unavailable-symbolic",
                    DeviceAction::Block,
                ));
            }
            DeviceState::Saved { .. } => {
                let send = icon_button("saved_devices_send_action", "document-send-symbolic");
                send.add_css_class("suggested-action");
                let (weak, target) = (Rc::downgrade(app), target.clone());
                send.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.compose_for_device(target.clone());
                    }
                });
                editor.controls.append(&send);
            }
            DeviceState::Blocked | DeviceState::Unavailable => {}
        }
        for (key, icon, action) in actions {
            let button = icon_button(key, icon);
            let destructive = matches!(action, DeviceAction::Forget | DeviceAction::Block);
            if matches!(action, DeviceAction::Remember | DeviceAction::Accept) {
                button.add_css_class("suggested-action");
                editor.primary.replace(Some(button.clone()));
            }
            if matches!(&action, DeviceAction::Label(label) if label.is_empty())
                && target.label.is_none()
            {
                button.set_sensitive(false);
            }
            if destructive {
                button.add_css_class("destructive-action");
            }
            let (weak_app, weak_editor, target) =
                (Rc::downgrade(app), Rc::downgrade(&editor), target.clone());
            button.connect_clicked(move |_| {
                let (Some(app), Some(_editor)) = (weak_app.upgrade(), weak_editor.upgrade()) else {
                    return;
                };
                let (target, action) = (target.clone(), action.clone());
                glib::spawn_future_local(async move {
                    if destructive {
                        let dialog = app.devices.borrow().as_ref().and_then(|view| {
                            view.management
                                .borrow()
                                .as_ref()
                                .map(|(dialog, _)| dialog.clone())
                        });
                        if let Some(dialog) = dialog {
                            dialog.force_close();
                        }
                        let (title, body) = match action {
                            DeviceAction::Forget => (
                                "saved_devices_forget_confirm_title",
                                "saved_devices_forget_confirm_body",
                            ),
                            DeviceAction::Block => (
                                "saved_devices_block_confirm_title",
                                "linux_block_device_body",
                            ),
                            _ => unreachable!(),
                        };
                        let alert = adw::AlertDialog::builder()
                            .heading(text(title))
                            .body(format(body, &[("device", &device_name(&target))]))
                            .build();
                        alert.add_responses(&[
                            ("cancel", &text("button_cancel")),
                            ("confirm", &text(key)),
                        ]);
                        alert.set_close_response("cancel");
                        alert.set_default_response(Some("cancel"));
                        alert.set_response_appearance(
                            "confirm",
                            adw::ResponseAppearance::Destructive,
                        );
                        if alert.choose_future(Some(&app.window)).await != "confirm" {
                            return;
                        }
                    }
                    app.device_action(target, action);
                });
            });
            editor.controls.append(&button);
        }
        editor
            .controls
            .set_sensitive(!app.device_busy.borrow().contains(&target.peer));
        editor
    }
}

fn device_name(device: &Device) -> String {
    device
        .name
        .clone()
        .unwrap_or_else(|| text("saved_devices_unnamed"))
}
