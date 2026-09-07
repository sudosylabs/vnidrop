use adw::prelude::*;

use super::i18n::text;

pub(super) fn protect_selection(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        if label.is_selectable() {
            // GTK 4.14 can retain a clipboard provider pointing to a disposed label.
            label.connect_destroy(|label| label.set_selectable(false));
        }
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        protect_selection(&widget);
        child = widget.next_sibling();
    }
}

pub(super) struct ProgressView {
    pub root: gtk::Box,
    label: gtk::Label,
    bar: gtk::ProgressBar,
    spinner: gtk::Spinner,
    statistics: gtk::Label,
    rate: std::cell::RefCell<vnidrop_gnome::progress::Rate>,
}

impl ProgressView {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .hexpand(true)
            .build();
        let spinner = gtk::Spinner::new();
        line.append(&label);
        line.append(&spinner);
        let bar = gtk::ProgressBar::new();
        root.append(&line);
        root.append(&bar);
        let statistics = gtk::Label::builder().xalign(0.0).wrap(true).build();
        statistics.add_css_class("dim-label");
        root.append(&statistics);
        Self {
            root,
            label,
            bar,
            spinner,
            statistics,
            rate: Default::default(),
        }
    }

    pub fn update(&self, progress: Option<&vnidrop_gnome::progress::Progress>) {
        let rate = self
            .rate
            .borrow_mut()
            .update(gtk::glib::monotonic_time() as u64 / 1000, progress);
        self.statistics.set_visible(rate.is_some());
        if let Some((bytes, seconds)) = rate {
            let speed = format!("{}/s", gtk::glib::format_size(bytes));
            let value = seconds
                .map(|seconds| {
                    super::i18n::format(
                        "linux_transfer_rate",
                        &[
                            ("rate", &speed),
                            ("time", &format!("{}:{:02}", seconds / 60, seconds % 60)),
                        ],
                    )
                })
                .unwrap_or(speed);
            self.statistics.set_label(&value);
        }
        self.root.set_visible(progress.is_some());
        let Some(progress) = progress else {
            self.spinner.stop();
            return;
        };
        let label = match (progress.bytes, progress.total) {
            (Some(bytes), Some(total)) => format!(
                "{} · {} / {}",
                text(progress.label),
                gtk::glib::format_size(bytes.min(total)),
                gtk::glib::format_size(total)
            ),
            (Some(bytes), None) => format!(
                "{} · {}",
                text(progress.label),
                gtk::glib::format_size(bytes)
            ),
            _ => text(progress.label),
        };
        self.label.set_label(&label);
        let fraction = progress.fraction();
        self.bar.set_visible(fraction.is_some());
        self.bar.set_fraction(fraction.unwrap_or(0.0));
        self.bar
            .update_property(&[gtk::accessible::Property::Label(&label)]);
        let waiting = fraction.is_none() && progress.label != "progress_interrupted";
        self.spinner.set_visible(waiting);
        self.spinner.set_spinning(waiting);
    }
}

pub(super) fn icon_button(key: &str, icon: &str) -> gtk::Button {
    let label = text(key);
    let content = adw::ButtonContent::builder()
        .icon_name(icon)
        .label(&label)
        .can_shrink(true)
        .build();
    let button = gtk::Button::builder().child(&content).build();
    button.update_property(&[gtk::accessible::Property::Label(&label)]);
    button
}

pub(super) fn set_icon_button_label(button: &gtk::Button, key: &str) {
    let label = text(key);
    button
        .child()
        .and_downcast::<adw::ButtonContent>()
        .unwrap()
        .set_label(&label);
    button.update_property(&[gtk::accessible::Property::Label(&label)]);
}
