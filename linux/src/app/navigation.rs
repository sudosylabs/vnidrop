use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use super::App;

impl App {
    pub(super) fn show_transfers(self: &Rc<Self>) {
        self.clear_review_highlight();
        self.showing_devices.set(false);
        self.render();
    }

    pub(super) fn review_transfer_request(self: &Rc<Self>) {
        if self.window.visible_dialog().is_some() {
            return;
        }
        let target = self.snapshot.borrow().as_ref().and_then(|snapshot| {
            snapshot
                .requests
                .iter()
                .filter(|r| r.status == "requested")
                .min_by_key(|r| (r.requested_at, &r.id))
                .map(|r| (r.transfer_id, r.id.clone()))
        });
        let Some((transfer_id, request_id)) = target else {
            return;
        };
        self.selected.replace(Some((transfer_id, "send".into())));
        self.show_transfers();
        self.split.set_show_content(true);
        self.reviewed_request.replace(Some(request_id.clone()));
        if let Some(button) = self.review_actions.borrow().get(&request_id) {
            self.focus_review(button);
        }
    }

    pub(super) fn review_device_request(self: &Rc<Self>) {
        if self.window.visible_dialog().is_some() {
            return;
        }
        let target = self.snapshot.borrow().as_ref().and_then(|snapshot| {
            snapshot
                .devices
                .list(glib::real_time() / 1000)
                .into_iter()
                .find(|device| {
                    device.needs_attention() && !self.device_busy.borrow().contains(&device.peer)
                })
        });
        let offer = self.snapshot.borrow().as_ref().and_then(|snapshot| {
            snapshot
                .offers
                .iter()
                .min_by_key(|o| o.received_at)
                .cloned()
        });
        let target = target.or_else(|| {
            offer.as_ref().and_then(|offer| {
                self.snapshot.borrow().as_ref().and_then(|snapshot| {
                    snapshot
                        .devices
                        .list(glib::real_time() / 1000)
                        .into_iter()
                        .find(|device| device.peer == offer.sender_endpoint_id)
                })
            })
        });
        self.show_devices();
        if let (Some(view), Some(target)) = (self.devices.borrow().clone(), target) {
            view.show_device(self, target);
            if let Some(button) = view
                .editor
                .borrow()
                .as_ref()
                .and_then(|editor| editor.primary.borrow().clone())
                .or_else(|| {
                    offer.as_ref().and_then(|offer| {
                        view.transfers.borrow().as_ref().and_then(|panel| {
                            panel.approvals.borrow().get(&offer.transfer_id).cloned()
                        })
                    })
                })
            {
                if let Some(offer) = offer.filter(|offer| {
                    view.transfers.borrow().as_ref().is_some_and(|panel| {
                        panel.approvals.borrow().get(&offer.transfer_id) == Some(&button)
                    })
                }) {
                    self.reviewed_offer.replace(Some(offer.transfer_id));
                }
                self.focus_review(&button);
            }
        }
    }

    pub(super) fn clear_review_highlight(&self) {
        self.reviewed_request.borrow_mut().take();
        self.reviewed_offer.borrow_mut().take();
        if let Some(widget) = self.review_highlight.borrow_mut().take() {
            widget.remove_css_class("review-highlight");
        }
    }

    pub(super) fn focus_review(&self, button: &gtk::Button) {
        if let Some(widget) = self.review_highlight.borrow_mut().take() {
            widget.remove_css_class("review-highlight");
        }
        let mut parent = button.parent();
        while let Some(widget) = parent {
            if widget.has_css_class("review-target") {
                widget.add_css_class("review-highlight");
                self.review_highlight.replace(Some(widget));
                break;
            }
            parent = widget.parent();
        }
        let card = self
            .review_highlight
            .borrow()
            .as_ref()
            .map(|widget| widget.downgrade());
        let button = button.downgrade();
        let frames = std::cell::Cell::new(0);
        // Let the narrow-page transition allocate its scroll view before revealing the action.
        self.window.add_tick_callback(move |_, _| {
            frames.set(frames.get() + 1);
            if frames.get() < 2 {
                return glib::ControlFlow::Continue;
            }
            if let Some(button) = button.upgrade().filter(|b| b.is_mapped()) {
                if frames.get() == 2 {
                    button.grab_focus();
                    return glib::ControlFlow::Continue;
                }
                if let (Some(card), Some(scroll)) = (
                    card.as_ref().and_then(|card| card.upgrade()),
                    button
                        .ancestor(gtk::ScrolledWindow::static_type())
                        .and_downcast::<gtk::ScrolledWindow>(),
                ) {
                    if let Some(bounds) = card.compute_bounds(&scroll) {
                        let height = scroll.height() as f32;
                        if bounds.height() + 24.0 <= height {
                            let delta = if bounds.y() < 12.0 {
                                bounds.y() - 12.0
                            } else {
                                (bounds.y() + bounds.height() + 12.0 - height).max(0.0)
                            };
                            let adjustment = scroll.vadjustment();
                            adjustment.set_value(adjustment.value() + f64::from(delta));
                        }
                    }
                }
            }
            glib::ControlFlow::Break
        });
        self.window.queue_draw();
    }
}
