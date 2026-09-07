use super::{i18n::text, App};
use adw::prelude::*;
use gtk::{gio, glib};
use std::rc::Rc;
use vnidrop_gnome::notifications::Target;

impl App {
    pub(super) fn setup_notifications(self: &Rc<Self>) {
        let action = gio::SimpleAction::new("notification-open", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        action.connect_activate(move |_, value| {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let Some(target) = value
                .and_then(|v| v.str())
                .and_then(|s| serde_json::from_str::<Target>(s).ok())
            else {
                return;
            };
            app.notification_target.replace(Some(target));
            app.window.present();
            app.refresh();
        });
        self.application.add_action(&action);
        let weak = Rc::downgrade(self);
        self.window.connect_visible_dialog_notify(move |_| {
            let weak = weak.clone();
            glib::idle_add_local_once(move || {
                if let Some(app) = weak.upgrade() {
                    app.open_notification_target();
                }
            });
        });
        let weak = Rc::downgrade(self);
        self.window.connect_is_active_notify(move |_| {
            if let Some(app) = weak.upgrade() {
                app.update_notifications();
            }
        });
    }
    pub(super) fn update_notifications(&self) {
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            return;
        };
        let (send, withdraw) = self.notifications.borrow_mut().update(
            snapshot,
            self.preferences.borrow().notifications,
            self.window.is_active(),
            glib::real_time() / 1000,
        );
        for id in withdraw {
            self.application.withdraw_notification(&id);
        }
        for notice in send {
            let notification = gio::Notification::new(&text(notice.title));
            notification.set_body(Some(&notice.body));
            notification.set_icon(&gio::ThemedIcon::new("com.vnidrop.VniDrop"));
            notification.set_priority(if notice.pending {
                gio::NotificationPriority::High
            } else {
                gio::NotificationPriority::Normal
            });
            let target = serde_json::to_string(&notice.target).unwrap().to_variant();
            notification
                .set_default_action_and_target_value("app.notification-open", Some(&target));
            self.application
                .send_notification(Some(&notice.id), &notification);
        }
    }
    pub(super) fn open_notification_target(self: &Rc<Self>) {
        if self.window.visible_dialog().is_some() || self.snapshot.borrow().is_none() {
            return;
        }
        let Some(target) = self.notification_target.borrow_mut().take() else {
            return;
        };
        match target {
            Target::Transfer(id, direction, request) => {
                self.selected.replace(Some((id, direction)));
                self.show_transfers();
                self.split.set_show_content(true);
                if let Some(request) = request {
                    let button = self.review_actions.borrow().get(&request).cloned();
                    if let Some(button) = button {
                        self.reviewed_request.replace(Some(request));
                        self.focus_review(&button);
                    }
                }
            }
            Target::Device(peer, offer) => {
                let device = self.snapshot.borrow().as_ref().and_then(|s| {
                    s.devices
                        .list(glib::real_time() / 1000)
                        .into_iter()
                        .find(|d| d.peer == peer)
                });
                self.show_devices();
                if let (Some(view), Some(device)) = (self.devices.borrow().clone(), device) {
                    view.show_device(self, device);
                    let button = offer
                        .as_ref()
                        .and_then(|id| {
                            view.transfers
                                .borrow()
                                .as_ref()
                                .and_then(|p| p.approvals.borrow().get(id).cloned())
                        })
                        .or_else(|| {
                            view.editor
                                .borrow()
                                .as_ref()
                                .and_then(|e| e.primary.borrow().clone())
                        });
                    if let Some(button) = button {
                        self.reviewed_offer.replace(offer);
                        self.focus_review(&button);
                    }
                }
            }
        }
    }
}

pub(super) async fn available() -> bool {
    let Ok(connection) = gio::bus_get_future(gio::BusType::Session).await else {
        return false;
    };
    for method in ["ListNames", "ListActivatableNames"] {
        if let Ok(result) = connection
            .call_future(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                method,
                None,
                None,
                gio::DBusCallFlags::NONE,
                3000,
            )
            .await
        {
            if let Some((names,)) = result.get::<(Vec<String>,)>() {
                if names.iter().any(|name| {
                    matches!(
                        name.as_str(),
                        "org.freedesktop.Notifications"
                            | "org.gtk.Notifications"
                            | "org.freedesktop.portal.Desktop"
                    )
                }) {
                    return true;
                }
            }
        }
    }
    false
}
