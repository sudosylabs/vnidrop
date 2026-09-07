use std::{cell::Cell, rc::Rc};

use adw::prelude::*;

use super::i18n::text;

pub(super) struct DeviceLayout {
    pub root: gtk::Paned,
    sidebar: adw::NavigationPage,
    content: adw::NavigationPage,
    sidebar_header: adw::HeaderBar,
    back: gtk::Button,
    collapsed: Cell<bool>,
    show_content: Cell<bool>,
}

impl DeviceLayout {
    pub fn new(
        sidebar: adw::NavigationPage,
        content: adw::NavigationPage,
        sidebar_header: adw::HeaderBar,
        content_header: &adw::HeaderBar,
    ) -> Rc<Self> {
        let back = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text(text("nav_saved_devices"))
            .build();
        content_header.pack_start(&back);
        content_header.set_show_start_title_buttons(false);
        let root = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&sidebar)
            .end_child(&content)
            .position(380)
            .resize_start_child(false)
            .shrink_start_child(false)
            .shrink_end_child(false)
            .build();
        let layout = Rc::new(Self {
            root,
            sidebar,
            content,
            sidebar_header,
            back,
            collapsed: Cell::new(false),
            show_content: Cell::new(false),
        });
        let weak = Rc::downgrade(&layout);
        layout.back.connect_clicked(move |_| {
            if let Some(layout) = weak.upgrade() {
                layout.set_show_content(false);
            }
        });
        layout.update();
        layout
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed.get()
    }

    pub fn set_collapsed(&self, collapsed: bool) {
        self.collapsed.set(collapsed);
        self.update();
    }

    pub fn set_show_content(&self, show: bool) {
        self.show_content.set(show);
        self.update();
    }

    fn update(&self) {
        let collapsed = self.is_collapsed();
        // Keep each pane rooted: GTK 4.14 can retain a realized accessibility context
        // after a modal closes, making NavigationSplitView's reparenting fail.
        self.sidebar
            .set_visible(!collapsed || !self.show_content.get());
        self.content
            .set_visible(!collapsed || self.show_content.get());
        self.sidebar
            .set_width_request(if collapsed { -1 } else { 280 });
        self.sidebar_header.set_show_end_title_buttons(collapsed);
        self.back.set_visible(collapsed);
    }
}
