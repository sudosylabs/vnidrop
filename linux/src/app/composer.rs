use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use adw::prelude::*;
use gtk::{gio, glib};
use vnidrop::{ShareMetadataInput, TransferAccessMode};
use vnidrop_gnome::{
    composer::{SelectionMode, TransferDraft},
    draft::{self, Preparation},
};

use super::{
    dialogs::content,
    i18n::{format, text},
    widgets::{icon_button, set_icon_button_label},
    App,
};

pub(super) struct Composer {
    app: Weak<App>,
    target: Option<vnidrop_gnome::devices::Device>,
    targeted_preparation: RefCell<Option<std::sync::Arc<vnidrop_gnome::targeted::Preparation>>>,
    pub(super) dialog: adw::Dialog,
    state: RefCell<TransferDraft>,
    name: adw::EntryRow,
    sender: adw::EntryRow,
    policy: adw::ComboRow,
    warning: gtk::Label,
    sources: gtk::ListBox,
    selection: adw::PreferencesGroup,
    files: gtk::Button,
    folder: gtk::Button,
    add: gtk::Button,
    clear: gtk::Button,
    submit: gtk::Button,
    cancel: gtk::Button,
    progress: gtk::Spinner,
    error: gtk::Label,
    syncing: Cell<bool>,
    preparation: RefCell<Option<std::sync::Arc<Preparation>>>,
}

fn multiple_files_name(count: usize) -> String {
    format(
        "send_default_transfer_name",
        &[("count", &count.to_string())],
    )
}

impl App {
    pub(super) fn compose(self: &Rc<Self>, files: Vec<gio::File>) {
        if self.session.borrow().is_none() || self.closing.get() {
            return;
        }
        if let Some(composer) = self.composer.borrow().as_ref() {
            composer.dialog.present(Some(&self.window));
            if !files.is_empty() {
                composer.load_files(files, SelectionMode::Add);
            }
            return;
        }
        // A draft must not displace a consent or invitation review already on screen.
        if self.window.visible_dialog().is_some() || self.reviewing.get() {
            return;
        }
        let composer = Composer::new(self, None);
        self.composer.replace(Some(composer.clone()));
        composer.dialog.present(Some(&self.window));
        if !files.is_empty() {
            composer.load_files(files, SelectionMode::Replace);
        }
    }
    pub(super) fn compose_for_device(self: &Rc<Self>, target: vnidrop_gnome::devices::Device) {
        if self.session.borrow().is_none()
            || self.closing.get()
            || self.window.visible_dialog().is_some()
        {
            return;
        }
        let composer = Composer::new(self, Some(target));
        self.composer.replace(Some(composer.clone()));
        composer.dialog.present(Some(&self.window));
    }
}

impl Composer {
    pub(super) fn choose_folder(self: &Rc<Self>) {
        self.pick(true, SelectionMode::Replace);
    }

    fn new(app: &Rc<App>, target: Option<vnidrop_gnome::devices::Device>) -> Rc<Self> {
        let (dialog, rows, header) = content("send_new_transfer_title");
        header.set_show_end_title_buttons(false);
        dialog.set_content_width(540);
        dialog.set_content_height(600);
        let name = adw::EntryRow::builder()
            .title(text("field_transfer_name"))
            .build();
        let sender = adw::EntryRow::builder()
            .title(text("field_sender_name"))
            .build();
        let naming = adw::PreferencesGroup::new();
        naming.add(&name);
        if target.is_none() {
            naming.add(&sender);
        }
        if let Some(target) = &target {
            naming.add(&super::dialogs::fact(
                "linux_targeted_recipient",
                target
                    .name
                    .as_deref()
                    .unwrap_or(&text("saved_devices_unnamed")),
            ));
        }
        rows.append(&naming);

        let selection = adw::PreferencesGroup::builder()
            .title(text("metadata_files"))
            .build();
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let add = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(text("linux_add_files"))
            .build();
        let clear = gtk::Button::builder().label(text("button_clear")).build();
        controls.append(&add);
        controls.append(&clear);
        selection.set_header_suffix(Some(&controls));
        let sources = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        sources.add_css_class("boxed-list");
        selection.add(&sources);
        rows.append(&selection);
        let pickers = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        pickers.set_homogeneous(true);
        let files = icon_button("button_choose_files", "document-open-symbolic");
        let folder = icon_button("button_choose_folder", "folder-open-symbolic");
        pickers.append(&files);
        pickers.append(&folder);
        rows.append(&pickers);

        let choices =
            gtk::StringList::new(&[&text("send_access_approval"), &text("send_access_anyone")]);
        let policy = adw::ComboRow::builder()
            .title(text("send_access_title"))
            .model(&choices)
            .use_subtitle(true)
            .build();
        let access = adw::PreferencesGroup::new();
        access.add(&policy);
        if target.is_none() {
            rows.append(&access);
        }
        let warning = gtk::Label::builder()
            .label(text("send_access_anyone_warning"))
            .wrap(true)
            .xalign(0.0)
            .build();
        warning.add_css_class("caption");
        rows.append(&warning);
        let error = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .visible(false)
            .build();
        error.add_css_class("error");
        rows.append(&error);
        let submit = gtk::Button::with_label(&text("linux_create_invitation"));
        submit.add_css_class("suggested-action");
        header.pack_end(&submit);
        let cancel = gtk::Button::with_label(&text("button_cancel"));
        header.pack_start(&cancel);
        let progress = gtk::Spinner::new();
        header.pack_end(&progress);
        let composer = Rc::new(Self {
            app: Rc::downgrade(app),
            target,
            targeted_preparation: RefCell::new(None),
            dialog,
            state: RefCell::new(TransferDraft::new(
                app.preferences.borrow().username.clone(),
            )),
            name,
            sender,
            policy,
            warning,
            sources,
            selection,
            files,
            folder,
            add,
            clear,
            submit,
            cancel,
            progress,
            error,
            syncing: Cell::new(false),
            preparation: RefCell::new(None),
        });
        composer.connect();
        composer.sync(true);
        composer
    }

    fn connect(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.name.connect_changed(move |entry| {
            if let Some(composer) = weak.upgrade().filter(|c| !c.syncing.get()) {
                composer
                    .state
                    .borrow_mut()
                    .change_transfer_name(entry.text().into());
                composer.sync(false);
            }
        });
        let weak = Rc::downgrade(self);
        self.sender.connect_changed(move |entry| {
            if let Some(composer) = weak.upgrade().filter(|c| !c.syncing.get()) {
                composer
                    .state
                    .borrow_mut()
                    .change_sender_name(entry.text().into());
            }
        });
        let weak = Rc::downgrade(self);
        self.policy.connect_selected_notify(move |row| {
            if let Some(composer) = weak.upgrade().filter(|c| !c.syncing.get()) {
                composer
                    .state
                    .borrow_mut()
                    .change_access_mode(if row.selected() == 0 {
                        TransferAccessMode::ApprovalRequired
                    } else {
                        TransferAccessMode::Public
                    });
                composer.sync(false);
            }
        });
        for (button, folder, mode) in [
            (&self.files, false, SelectionMode::Replace),
            (&self.folder, true, SelectionMode::Replace),
            (&self.add, false, SelectionMode::Add),
        ] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(composer) = weak.upgrade() {
                    composer.pick(folder, mode);
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.clear.connect_clicked(move |_| {
            if let Some(composer) = weak.upgrade() {
                composer.state.borrow_mut().clear_sources();
                composer.sync(true);
            }
        });
        let weak = Rc::downgrade(self);
        self.submit.connect_clicked(move |_| {
            if let Some(composer) = weak.upgrade() {
                composer.submit();
            }
        });
        let weak = Rc::downgrade(self);
        self.cancel.connect_clicked(move |_| {
            if let Some(composer) = weak.upgrade() {
                composer.cancel();
            }
        });
        let weak = Rc::downgrade(self);
        self.dialog.connect_closed(move |_| {
            if let Some(composer) = weak.upgrade() {
                composer.state.borrow_mut().dismiss();
                if let Some(app) = composer.app.upgrade() {
                    app.composer.borrow_mut().take();
                    app.pump_invitations();
                }
            }
        });
    }

    fn sync(self: &Rc<Self>, rebuild_sources: bool) {
        self.syncing.set(true);
        let state = self.state.borrow();
        if self.name.text() != state.transfer_name() {
            self.name.set_text(state.transfer_name());
        }
        if self.sender.text() != state.sender_name() {
            self.sender.set_text(state.sender_name());
        }
        self.policy
            .set_selected(if state.access_mode() == TransferAccessMode::Public {
                1
            } else {
                0
            });
        self.warning.set_visible(
            self.target.is_none() && state.access_mode() == TransferAccessMode::Public,
        );
        self.name.set_sensitive(state.editable());
        self.sender.set_sensitive(state.editable());
        self.policy.set_sensitive(state.editable());
        self.files.set_sensitive(state.editable());
        self.folder.set_sensitive(state.editable());
        self.add.set_sensitive(state.editable());
        self.clear.set_sensitive(state.editable());
        self.sources.set_sensitive(state.editable());
        self.selection.set_visible(!state.sources().is_empty());
        self.add
            .set_visible(!state.sources().iter().any(|item| item.source.is_directory));
        set_icon_button_label(
            &self.files,
            if state.sources().is_empty() {
                "button_choose_files"
            } else {
                "button_change_files"
            },
        );
        self.submit.set_sensitive(state.can_submit());
        self.submit.set_label(&text(if state.is_submitting() {
            "button_sharing_file"
        } else if self.target.is_some() {
            "saved_devices_send_action"
        } else {
            "linux_create_invitation"
        }));
        self.dialog.set_can_close(!state.is_submitting());
        self.progress.set_visible(!state.editable());
        self.progress.set_spinning(!state.editable());
        if rebuild_sources {
            // Selection edits remove focused buttons; move focus to a stable control first.
            self.files.grab_focus();
            while let Some(child) = self.sources.first_child() {
                self.sources.remove(&child);
            }
            for item in state.sources() {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(
                        item.source.display_name.as_deref().unwrap_or(""),
                    ))
                    .title_lines(2)
                    .build();
                let icon = if item.source.is_directory {
                    gio::ThemedIcon::new("folder-symbolic").upcast::<gio::Icon>()
                } else {
                    let (content_type, _) = gio::content_type_guess(Some(&item.source.value), None);
                    gio::content_type_get_symbolic_icon(&content_type)
                };
                row.add_prefix(&gtk::Image::from_gicon(&icon));
                let remove = gtk::Button::builder()
                    .icon_name("list-remove-symbolic")
                    .tooltip_text(text("button_remove_file"))
                    .valign(gtk::Align::Center)
                    .build();
                remove.add_css_class("flat");
                let id = item.id;
                let weak = Rc::downgrade(self);
                remove.connect_clicked(move |_| {
                    if let Some(composer) = weak.upgrade() {
                        composer
                            .state
                            .borrow_mut()
                            .remove_source(id, multiple_files_name);
                        composer.sync(true);
                    }
                });
                row.add_suffix(&remove);
                self.sources.append(&row);
            }
        }
        self.syncing.set(false);
    }

    fn pick(self: &Rc<Self>, folder: bool, mode: SelectionMode) {
        let Some(request) = self.state.borrow_mut().begin_pick(mode) else {
            return;
        };
        self.sync(false);
        let composer = self.clone();
        glib::spawn_future_local(async move {
            let Some(app) = composer.app.upgrade() else {
                return;
            };
            let picker = gtk::FileDialog::builder()
                .title(text(if folder {
                    "button_choose_folder"
                } else {
                    "button_choose_files"
                }))
                .build();
            let result = if folder {
                picker
                    .select_folder_future(Some(&app.window))
                    .await
                    .map(|file| vec![file])
            } else {
                picker
                    .open_multiple_future(Some(&app.window))
                    .await
                    .map(|files| {
                        (0..files.n_items())
                            .filter_map(|i| files.item(i).and_downcast::<gio::File>())
                            .collect()
                    })
            };
            match result {
                Ok(files) => composer.resolve_files(request, files).await,
                Err(error) if error.matches(gtk::DialogError::Dismissed) => {
                    composer.finish_pick(request, Ok(vec![]))
                }
                Err(_) => composer.finish_pick(request, Err("error_selection_failed")),
            }
        });
    }

    pub(super) fn load_files(self: &Rc<Self>, files: Vec<gio::File>, mode: SelectionMode) {
        let Some(request) = self.state.borrow_mut().begin_pick(mode) else {
            return;
        };
        self.sync(false);
        let composer = self.clone();
        glib::spawn_future_local(async move {
            composer.resolve_files(request, files).await;
        });
    }

    async fn resolve_files(self: &Rc<Self>, request: u64, files: Vec<gio::File>) {
        let paths: Option<Vec<_>> = files.iter().map(gio::File::path).collect();
        let result = match paths {
            Some(paths) if paths.is_empty() => Ok(vec![]),
            Some(paths) => gio::spawn_blocking(move || draft::sources(paths))
                .await
                .unwrap_or(Err("error_generic")),
            None => Err("linux_local_files_only"),
        };
        self.finish_pick(request, result);
    }

    fn finish_pick(
        self: &Rc<Self>,
        request: u64,
        result: vnidrop_gnome::error::Result<Vec<vnidrop::ShareSource>>,
    ) {
        let result = self
            .state
            .borrow_mut()
            .complete_pick(request, result, multiple_files_name);
        self.error.set_visible(result.is_err());
        if let Err(key) = result {
            self.error.set_label(&text(key));
        }
        self.sync(true);
    }

    fn submit(self: &Rc<Self>) {
        let Some(app) = self.app.upgrade().filter(|app| !app.closing.get()) else {
            return;
        };
        let Some(submission) = self.state.borrow_mut().begin_submission() else {
            return;
        };
        if let Some(target) = self.target.clone() {
            self.submit_targeted(&app, target, submission);
            return;
        }
        let preview_source = (submission.sources.len() == 1 && !submission.sources[0].is_directory)
            .then(|| std::path::PathBuf::from(&submission.sources[0].value));
        let preparation = Preparation::new();
        let metadata = ShareMetadataInput {
            transfer_id: preparation.id,
            transfer_name: Some(submission.transfer_name),
            sender_name: Some(submission.sender_name),
            access_mode: submission.access_mode,
        };
        self.preparation.replace(Some(preparation.clone()));
        self.error.set_visible(false);
        self.sync(false);
        let composer = self.clone();
        let completion = preparation.clone();
        app.dispatch(
            move |session| preparation.run(&session, submission.sources, metadata),
            move |app, result| {
                let result = if completion.is_cancelled() {
                    Err("progress_cancelled")
                } else {
                    result
                };
                composer.preparation.borrow_mut().take();
                composer
                    .state
                    .borrow_mut()
                    .finish_submission(result.is_ok());
                composer.cancel.set_sensitive(true);
                match result {
                    Ok(share) => {
                        if let Some(source) = preview_source {
                            app.save_preview(share.transfer_id, source);
                        }
                        app.selected
                            .replace(Some((share.transfer_id, "send".into())));
                        composer.dialog.set_can_close(true);
                        composer.dialog.close();
                        app.show_transfers();
                        app.split.set_show_content(true);
                    }
                    Err(key) => {
                        composer.error.set_label(&text(key));
                        composer.error.set_visible(key != "progress_cancelled");
                        composer.sync(false);
                    }
                }
                app.refresh();
            },
        );
    }

    fn submit_targeted(
        self: &Rc<Self>,
        app: &Rc<App>,
        target: vnidrop_gnome::devices::Device,
        submission: vnidrop_gnome::composer::Submission,
    ) {
        let preparation = vnidrop_gnome::targeted::Preparation::new();
        self.targeted_preparation.replace(Some(preparation.clone()));
        self.error.set_visible(false);
        self.sync(false);
        let composer = self.clone();
        let completion = preparation.clone();
        app.dispatch(
            move |session| preparation.run(&session, &target, submission),
            move |app, result| {
                let result = if completion.is_cancelled() {
                    Err("progress_cancelled")
                } else {
                    result
                };
                composer.targeted_preparation.borrow_mut().take();
                composer
                    .state
                    .borrow_mut()
                    .finish_submission(result.is_ok());
                composer.cancel.set_sensitive(true);
                match result {
                    Ok(_) => {
                        composer.dialog.set_can_close(true);
                        composer.dialog.close();
                        app.error("saved_devices_send_started");
                    }
                    Err(key) => {
                        composer.error.set_label(&text(key));
                        composer.error.set_visible(key != "progress_cancelled");
                        composer.sync(false);
                    }
                }
                app.refresh();
            },
        );
    }

    fn cancel(self: &Rc<Self>) {
        if let Some(preparation) = self.targeted_preparation.borrow().clone() {
            preparation.request_cancel();
            self.cancel.set_sensitive(false);
            if let Some(app) = self.app.upgrade() {
                app.dispatch(
                    move |_| preparation.stop(),
                    |app, result| {
                        if let Err(key) = result {
                            app.error(key);
                        }
                        app.refresh();
                    },
                );
            }
            return;
        }
        let Some(preparation) = self.preparation.borrow().clone() else {
            self.dialog.close();
            return;
        };
        let Some(app) = self.app.upgrade() else {
            return;
        };
        preparation.request_cancel();
        self.cancel.set_sensitive(false);
        app.dispatch(
            move |session| {
                preparation.cancel(&session);
                Ok(())
            },
            |app, _| app.refresh(),
        );
    }
}
