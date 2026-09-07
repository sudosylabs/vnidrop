use super::*;

fn combo(widget: &gtk::Widget, title: &str) -> Option<adw::ComboRow> {
    if let Some(row) = widget.downcast_ref::<adw::ComboRow>() {
        if row.title() == title {
            return Some(row.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(row) = combo(&widget, title) {
            return Some(row);
        }
        child = widget.next_sibling();
    }
    None
}
pub(super) fn settings_restart_and_preview_workflow(root: &std::path::Path) {
    super::super::previews::verify_preview_cache();
    let application = adw::Application::builder()
        .application_id("com.vnidrop.VniDrop.SettingsUITest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let profile = root.join("settings-profile");
    let downloads = root.join("settings-downloads");
    let mut preferences = Preferences::defaults(downloads.clone(), "Settings test".into());
    preferences.network.mode = vnidrop::CoreRelayMode::LocalOnly;
    preferences.save(&profile).unwrap();
    let app = App::new(&application, profile.clone());
    app.window.present();
    app.initialize();
    until("settings initialized", || {
        app.snapshot.borrow().is_some() && app.busy.get() == 0
    });
    let identity = app
        .session
        .borrow()
        .as_ref()
        .unwrap()
        .call(|core| Ok(core.status().endpoint_id))
        .unwrap();
    app.show_preferences();
    let dialog = app.window.visible_dialog().unwrap();
    entry(&dialog, &text("field_username"))
        .unwrap()
        .set_text("Native preferences");
    combo(dialog.upcast_ref(), &text("appearance_title"))
        .unwrap()
        .set_selected(2);
    until("theme persists without Save", || {
        !app.saving_preferences.get() && app.preferences.borrow().theme == "Dark"
    });
    assert_eq!(
        app.application.style_manager().color_scheme(),
        adw::ColorScheme::ForceDark
    );
    render_window(&app.window);
    dialog.close();
    until("theme dialog closed", || {
        app.window.visible_dialog().is_none()
    });
    app.show_preferences();
    let dialog = app.window.visible_dialog().unwrap();
    assert_eq!(
        combo(dialog.upcast_ref(), &text("appearance_title"))
            .unwrap()
            .selected(),
        2
    );
    entry(&dialog, &text("field_username"))
        .unwrap()
        .set_text("Native preferences");
    capture(&app.window, "preferences-wide");
    app.window.set_default_size(390, 700);
    until("narrow preferences", || app.window.width() < 500);
    capture(&app.window, "preferences-narrow");
    activate_button(&dialog, "linux_save_preferences");
    until("preferences saved", || {
        !app.saving_preferences.get()
            && app.preferences.borrow().username == "Native preferences"
            && app.window.visible_dialog().is_none()
    });
    assert_eq!(
        Preferences::load(&profile, preferences.clone())
            .unwrap()
            .theme,
        "Dark"
    );
    assert!(downloads.is_dir());
    app.show_preferences();
    let dialog = app.window.visible_dialog().unwrap();
    let mode = combo(dialog.upcast_ref(), &text("linux_network_policy")).unwrap();
    for selection in 0..4 {
        mode.set_selected(selection);
        render_window(&app.window);
        assert!(mode.uses_subtitle());
        assert_eq!(mode.subtitle_lines(), 0);
        assert!(mode.list_factory().is_some());
    }
    capture(&app.window, "preferences-long-selection");
    mode.emit_by_name::<()>("activate", &[]);
    capture(&app.window, "preferences-options");
    fn check_labels(widget: &gtk::Widget, visible: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if label.is_mapped() {
                visible.push(label.text().to_string());
            }
            if label.is_mapped()
                && label
                    .text()
                    .contains(&text("relay_mode_custom_direct_fallback"))
            {
                assert!(
                    !label.layout().is_ellipsized(),
                    "network option must be fully visible"
                );
            }
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            check_labels(&widget, visible);
            child = widget.next_sibling();
        }
    }
    let mut visible = Vec::new();
    check_labels(dialog.upcast_ref(), &mut visible);
    for key in [
        "relay_mode_automatic",
        "relay_mode_local_only",
        "relay_mode_custom",
        "relay_mode_custom_direct_fallback",
    ] {
        assert!(
            visible.contains(&text(key)),
            "dropdown option visible: {key}"
        );
    }
    fn close_popovers(widget: &gtk::Widget) {
        if let Some(popover) = widget.downcast_ref::<gtk::Popover>() {
            popover.popdown();
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            close_popovers(&widget);
            child = widget.next_sibling();
        }
    }
    close_popovers(mode.upcast_ref());
    render_window(&app.window);
    mode.set_selected(1);
    let old_session = app.session.borrow().as_ref().unwrap().clone();
    render_window(&app.window);
    activate_button(&dialog, "relay_apply");
    until("network restart complete", || {
        app.session
            .borrow()
            .as_ref()
            .is_some_and(|current| !Arc::ptr_eq(current, &old_session))
            && !app.reconfiguring.get()
            && app.session.borrow().is_some()
            && app.window.visible_dialog().is_none()
    });
    assert_eq!(
        app.session
            .borrow()
            .as_ref()
            .unwrap()
            .call(|core| Ok(core.status().endpoint_id))
            .unwrap(),
        identity
    );
    until("network snapshot settled", || app.busy.get() == 0);
    let old_session = app.session.borrow().as_ref().unwrap().clone();
    app.show_preferences();
    render_window(&app.window);
    activate_button(
        &app.window.visible_dialog().unwrap(),
        "storage_clear_transfer_cache",
    );
    until("cache confirmation", || {
        app.window
            .visible_dialog()
            .is_some_and(|d| d.is::<adw::AlertDialog>())
    });
    render_window(&app.window);
    activate_button(
        &app.window.visible_dialog().unwrap(),
        "storage_clear_transfer_cache",
    );
    until("cache restart complete", || {
        app.session
            .borrow()
            .as_ref()
            .is_some_and(|current| !Arc::ptr_eq(current, &old_session))
            && !app.reconfiguring.get()
            && app.session.borrow().is_some()
            && app.window.visible_dialog().is_none()
    });
    until("cache snapshot settled", || app.busy.get() == 0);
    let source = root.join("review-source.txt");
    std::fs::write(&source, b"review").unwrap();
    let share = app
        .session
        .borrow()
        .as_ref()
        .unwrap()
        .call(|core| {
            core.share_files(
                vnidrop_gnome::draft::sources(vec![source.clone()]).unwrap(),
                vnidrop::ShareMetadataInput {
                    transfer_id: 990,
                    transfer_name: Some("Review files".into()),
                    sender_name: Some("Native".into()),
                    access_mode: vnidrop::TransferAccessMode::ApprovalRequired,
                },
            )
        })
        .unwrap();
    app.preferences.borrow_mut().receive_directory = source;
    app.review_saved_invitation(share.ticket);
    until("receive review", || app.window.visible_dialog().is_some());
    let review = app.window.visible_dialog().unwrap();
    render_window(&app.window);
    let name = entry(&review, &text("field_username")).unwrap();
    name.set_text("Edited receiver");
    activate_button(&review, "button_receive_files");
    until("invalid destination leaves review editable", || {
        button(&review, &|b| {
            b.label().as_deref() == Some(&text("button_receive_files"))
        })
        .unwrap()
        .is_sensitive()
    });
    assert!(app.window.visible_dialog().is_some());
    assert_eq!(name.text(), "Edited receiver");
    assert!(app.receive_drafts.borrow().is_empty());
    capture(&app.window, "receive-review-invalid-destination");
    review.close();
    until("receive review closed", || {
        app.window.visible_dialog().is_none()
    });
    app.preferences.borrow_mut().receive_directory = downloads;
    app.session
        .borrow()
        .as_ref()
        .unwrap()
        .call(|core| core.cancel_transfer(990))
        .unwrap();
    app.refresh();
    until("review share stopped", || {
        app.busy.get() == 0 && !app.snapshot.borrow().as_ref().unwrap().has_obligations()
    });
    app.show_about();
    let about = app.window.visible_dialog().unwrap();
    capture(&app.window, "about-narrow");
    activate_button(&about, "about_bug_report");
    until("About opens report", || {
        app.window
            .visible_dialog()
            .is_some_and(|d| d.title() == text("about_bug_report"))
    });
    capture(&app.window, "bug-report-narrow");
    let dialog = app.window.visible_dialog().unwrap();
    assert!(button(&dialog, &|b| b
        .child()
        .and_downcast::<adw::ButtonContent>()
        .is_some_and(|c| c.label() == text("bug_report_submit")))
    .is_some());
    dialog.close();
    until("report closed", || app.window.visible_dialog().is_none());
    let image_path = root.join("preview.png");
    let image =
        gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 256, 128).unwrap();
    image.fill(0x6699ccff);
    image.savev(&image_path, "png", &[]).unwrap();
    app.application.activate_action("send", None);
    let draft = app.window.visible_dialog().unwrap();
    let submit = button(&draft, &|b| {
        b.label().as_deref() == Some(&text("linux_create_invitation"))
    })
    .unwrap();
    app.composer.borrow().as_ref().unwrap().load_files(
        vec![gio::File::for_path(&image_path)],
        vnidrop_gnome::composer::SelectionMode::Replace,
    );
    until("image selected", || submit.is_sensitive());
    submit.emit_clicked();
    until("image preview visible", || {
        !app.previews.borrow().is_empty() && app.window.visible_dialog().is_none()
    });
    let id = *app.previews.borrow().keys().next().unwrap();
    app.selected.replace(Some((id, "send".into())));
    app.rows.borrow_mut().clear();
    app.render();
    fn has_texture(widget: &gtk::Widget) -> bool {
        if widget
            .downcast_ref::<gtk::Image>()
            .and_then(|image| image.paintable())
            .is_some_and(|p| p.is::<gtk::gdk::Texture>())
        {
            return true;
        }
        if widget
            .downcast_ref::<gtk::Picture>()
            .and_then(|picture| picture.paintable())
            .is_some_and(|p| p.is::<gtk::gdk::Texture>())
        {
            return true;
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if has_texture(&widget) {
                return true;
            }
            child = widget.next_sibling();
        }
        false
    }
    assert!(
        has_texture(app.list.upcast_ref()),
        "rebuilt row retains image preview"
    );
    assert!(
        has_texture(app.details.upcast_ref()),
        "transfer details displays the image"
    );
    capture(&app.window, "image-transfer-preview");
    app.session
        .borrow()
        .as_ref()
        .unwrap()
        .call(|core| core.cancel_transfer(id))
        .unwrap();
    app.refresh();
    until("image share stopped", || {
        app.busy.get() == 0 && !app.snapshot.borrow().as_ref().unwrap().has_obligations()
    });
    until("settings idle", || app.busy.get() == 0);
    app.request_close();
    until("settings window closed", || !app.window.is_visible());
}
