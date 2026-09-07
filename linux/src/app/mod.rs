mod details;
mod dialogs;
mod i18n;

#[cfg(test)]
mod tests;

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use adw::prelude::*;
use gtk::{gio, glib};
use vnidrop_gnome::{
    error::Result,
    preferences::Preferences,
    session::{Session, Snapshot},
};

use i18n::text;

pub(super) struct App {
    application: adw::Application,
    window: adw::ApplicationWindow,
    builder: gtk::Builder,
    split: adw::NavigationSplitView,
    list: gtk::ListBox,
    details: gtk::Box,
    toasts: adw::ToastOverlay,
    profile: PathBuf,
    preferences: RefCell<Preferences>,
    session: RefCell<Option<Arc<Session>>>,
    snapshot: RefCell<Option<Snapshot>>,
    selected: RefCell<Option<(u64, String)>>,
    refreshing: Cell<bool>,
    refresh_again: Cell<bool>,
    busy: Cell<usize>,
    closing: Cell<bool>,
    confirming_close: Cell<bool>,
    reviewing: Cell<bool>,
    pending: RefCell<VecDeque<gio::File>>,
    rows: RefCell<Vec<details::TransferRow>>,
    detail_fingerprint: RefCell<String>,
    preference_queue: RefCell<VecDeque<Preferences>>,
    saving_preferences: Cell<bool>,
}

pub fn run() -> glib::ExitCode {
    let mut args = std::env::args().collect::<Vec<_>>();
    let mut profile = glib::home_dir().join(".vnidrop");
    let mut custom_profile = false;
    let mut argument_error = false;
    if let Some(index) = args.iter().position(|arg| arg == "--profile") {
        args.remove(index);
        if index < args.len() {
            profile = PathBuf::from(args.remove(index));
            custom_profile = true;
            argument_error = !profile.is_absolute();
        } else {
            argument_error = true;
        }
    }
    if argument_error {
        eprintln!("{}", text("linux_profile_arguments"));
        return glib::ExitCode::FAILURE;
    }
    let app_id = if custom_profile {
        let identity =
            std::fs::create_dir_all(&profile).and_then(|()| std::fs::canonicalize(&profile));
        let Ok(identity) = identity else {
            eprintln!("{}", text("error_filesystem"));
            return glib::ExitCode::FAILURE;
        };
        profile = identity;
        if std::fs::canonicalize(glib::home_dir().join(".vnidrop"))
            .ok()
            .as_ref()
            == Some(&profile)
        {
            "com.vnidrop.VniDrop".into()
        } else {
            format!(
                "com.vnidrop.VniDrop.Profile{}",
                &blake3::hash(profile.as_os_str().as_encoded_bytes()).to_hex()[..16]
            )
        }
    } else {
        "com.vnidrop.VniDrop".into()
    };
    let application = adw::Application::builder()
        .application_id(app_id)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let holder: Rc<RefCell<Option<Rc<App>>>> = Rc::default();
    let startup_holder = holder.clone();
    application.connect_startup(move |application| {
        gio::resources_register_include!("icons.gresource").expect("compiled icon resources");
        gtk::IconTheme::for_display(&gtk::gdk::Display::default().expect("GTK display"))
            .add_resource_path("/com/vnidrop/VniDrop/icons");
        gtk::Window::set_default_icon_name("com.vnidrop.VniDrop");
        let app = App::new(application, profile.clone());
        app.initialize();
        startup_holder.replace(Some(app));
    });
    let activate_holder = holder.clone();
    application.connect_activate(move |_| {
        if let Some(app) = activate_holder.borrow().as_ref() {
            app.window.present();
        }
    });
    let open_holder = holder.clone();
    application.connect_open(move |_, files, _| {
        if let Some(app) = open_holder.borrow().as_ref() {
            app.window.present();
            for file in files {
                app.enqueue_invitation(file.clone());
            }
        }
    });
    application.connect_shutdown(move |_| {
        holder.borrow_mut().take();
    });
    application.run_with_args(&args)
}

impl App {
    fn new(application: &adw::Application, profile: PathBuf) -> Rc<Self> {
        let builder = gtk::Builder::from_string(include_str!("window.ui"));
        let window: adw::ApplicationWindow = builder.object("window").unwrap();
        window.set_application(Some(application));
        let split: adw::NavigationSplitView = builder.object("split").unwrap();
        let breakpoint =
            adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 700sp").unwrap());
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(breakpoint);
        let downloads = glib::user_special_dir(glib::UserDirectory::Downloads)
            .unwrap_or_else(|| glib::home_dir().join("Downloads"));
        let app = Rc::new(Self {
            application: application.clone(),
            window,
            split,
            list: builder.object("transfers").unwrap(),
            details: builder.object("details").unwrap(),
            toasts: builder.object("toasts").unwrap(),
            builder,
            profile,
            preferences: RefCell::new(Preferences::defaults(
                downloads,
                glib::host_name().to_string(),
            )),
            session: RefCell::new(None),
            snapshot: RefCell::new(None),
            selected: RefCell::new(None),
            refreshing: Cell::new(false),
            refresh_again: Cell::new(false),
            busy: Cell::new(0),
            closing: Cell::new(false),
            confirming_close: Cell::new(false),
            reviewing: Cell::new(false),
            pending: RefCell::new(VecDeque::new()),
            rows: RefCell::new(Vec::new()),
            detail_fingerprint: RefCell::new(String::new()),
            preference_queue: RefCell::new(VecDeque::new()),
            saving_preferences: Cell::new(false),
        });
        app.object::<adw::NavigationPage>("sidebar_page")
            .set_title(&text("linux_transfers"));
        app.object::<adw::NavigationPage>("detail_page")
            .set_title(&text("transfer_details_title"));
        app.object::<adw::StatusPage>("empty")
            .set_title(&text("linux_empty_title"));
        app.object::<adw::StatusPage>("empty")
            .set_description(Some(&text("linux_empty_body")));
        app.object::<gtk::Button>("empty_send")
            .set_label(&text("linux_send_files"));
        app.object::<gtk::Button>("send")
            .set_tooltip_text(Some(&text("linux_send_files")));
        app.object::<gtk::Button>("retry")
            .set_label(&text("button_retry"));
        app.object::<gtk::MenuButton>("menu")
            .set_tooltip_text(Some(&text("button_more_actions")));
        app.actions();
        let banner = app.object::<adw::Banner>("requests");
        banner.set_button_label(Some(&text("linux_review_requests")));
        banner.set_action_name(Some("app.review-requests"));
        let welcome_header = adw::HeaderBar::new();
        let welcome_menu = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text(text("button_more_actions"))
            .menu_model(&app.object::<gtk::MenuButton>("menu").menu_model().unwrap())
            .build();
        welcome_header.pack_end(&welcome_menu);
        let welcome_actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .halign(gtk::Align::Center)
            .build();
        let send = gtk::Button::builder()
            .label(text("linux_send_files"))
            .action_name("app.send")
            .build();
        send.add_css_class("suggested-action");
        send.add_css_class("pill");
        let open = gtk::Button::builder()
            .label(text("linux_open_invitation"))
            .action_name("app.open")
            .build();
        open.add_css_class("pill");
        welcome_actions.append(&send);
        welcome_actions.append(&open);
        let welcome = adw::StatusPage::builder()
            .title(text("linux_empty_title"))
            .description(text("linux_empty_body"))
            .icon_name("folder-download-symbolic")
            .child(&welcome_actions)
            .build();
        let welcome_view = adw::ToolbarView::builder().content(&welcome).build();
        welcome_view.add_top_bar(&welcome_header);
        app.object::<gtk::Stack>("root")
            .add_named(&welcome_view, Some("welcome"));
        let weak = Rc::downgrade(&app);
        app.list.connect_row_activated(move |_, row| {
            if let Some(app) = weak.upgrade() {
                let key = app
                    .snapshot
                    .borrow()
                    .as_ref()
                    .and_then(|snapshot| snapshot.transfers.get(row.index() as usize))
                    .map(|t| (t.transfer_id, t.direction.clone()));
                app.selected.replace(key);
                app.render_details();
                app.split.set_show_content(true);
            }
        });
        let weak = Rc::downgrade(&app);
        app.window.connect_close_request(move |_| {
            if let Some(app) = weak.upgrade() {
                app.request_close();
            }
            glib::Propagation::Stop
        });
        let drop_target = gtk::DropTarget::new(
            gtk::gdk::FileList::static_type(),
            gtk::gdk::DragAction::COPY,
        );
        let weak = Rc::downgrade(&app);
        drop_target.connect_drop(move |_, value, _, _| {
            let (Some(app), Ok(files)) = (weak.upgrade(), value.get::<gtk::gdk::FileList>()) else {
                return false;
            };
            if app.session.borrow().is_none() {
                return false;
            }
            app.compose(files.files());
            true
        });
        app.window.add_controller(drop_target);
        app.render_details();
        app
    }

    fn object<T: IsA<glib::Object>>(&self, id: &str) -> T {
        self.builder.object(id).expect("window template object")
    }

    fn action(self: &Rc<Self>, name: &str, callback: impl Fn(Rc<Self>) + 'static) {
        let action = gio::SimpleAction::new(name, None);
        let weak = Rc::downgrade(self);
        action.connect_activate(move |_, _| {
            if let Some(app) = weak.upgrade() {
                callback(app);
            }
        });
        self.application.add_action(&action);
    }

    fn actions(self: &Rc<Self>) {
        self.action("review-requests", |app| {
            let id = app
                .snapshot
                .borrow()
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .requests
                        .iter()
                        .find(|request| request.status == "requested")
                })
                .map(|request| request.transfer_id);
            if let Some(id) = id {
                app.selected.replace(Some((id, "send".into())));
                app.render();
                app.split.set_show_content(true);
            }
        });
        self.action("send", |app| app.pick_sources(false));
        self.action("send-folder", |app| app.pick_sources(true));
        self.action("open", |app| app.pick_invitation());
        self.action("preferences", |app| app.show_preferences());
        self.action("quit", |app| app.request_close());
        self.action("retry", |app| app.initialize());
        self.action("about", |app| {
            adw::AboutDialog::builder()
                .application_name("VniDrop")
                .application_icon("com.vnidrop.VniDrop")
                .developer_name("Sudosy Labs")
                .version(
                    include_str!("../../../version.properties")
                        .lines()
                        .find_map(|line| line.strip_prefix("PRODUCT_VERSION="))
                        .unwrap_or(env!("CARGO_PKG_VERSION")),
                )
                .comments(text("linux_application_description"))
                .website("https://github.com/vnidrop/vnidrop")
                .license_type(gtk::License::Apache20)
                .build()
                .present(Some(&app.window));
        });
        let menu = gio::Menu::new();
        let files = gio::Menu::new();
        files.append(Some(&text("linux_send_folder")), Some("app.send-folder"));
        files.append(Some(&text("linux_open_invitation")), Some("app.open"));
        menu.append_section(None, &files);
        let settings = gio::Menu::new();
        settings.append(Some(&text("preferences_title")), Some("app.preferences"));
        settings.append(Some(&text("linux_about")), Some("app.about"));
        menu.append_section(None, &settings);
        menu.append(Some(&text("linux_quit")), Some("app.quit"));
        self.object::<gtk::MenuButton>("menu")
            .set_menu_model(Some(&menu));
        self.application
            .set_accels_for_action("app.send", &["<Primary>n"]);
        self.application
            .set_accels_for_action("app.open", &["<Primary>o"]);
        self.application
            .set_accels_for_action("app.preferences", &["<Primary>comma"]);
        self.application
            .set_accels_for_action("app.quit", &["<Primary>q"]);
        self.set_ready(false);
    }

    fn set_ready(&self, ready: bool) {
        for name in ["send", "send-folder", "open", "preferences"] {
            self.application
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
                .unwrap()
                .set_enabled(ready);
        }
    }

    fn initialize(self: &Rc<Self>) {
        if self.busy.get() != 0 || self.session.borrow().is_some() || self.closing.get() {
            return;
        }
        self.object::<adw::StatusPage>("startup")
            .set_title("VniDrop");
        self.object::<adw::StatusPage>("startup")
            .set_description(Some(&text("app_starting")));
        self.object::<gtk::Spinner>("spinner").set_visible(true);
        self.object::<gtk::Button>("retry").set_visible(false);
        self.busy.set(1);
        let profile = self.profile.clone();
        let defaults = self.preferences.borrow().clone();
        let app = self.clone();
        let hold = self.application.hold();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                let preferences = Preferences::load(&profile, defaults)?;
                let path = profile.to_str().ok_or("linux_local_files_only")?.to_owned();
                let session = Session::open(path, preferences.network.clone())?;
                Ok((preferences, session))
            })
            .await
            .unwrap_or(Err("error_generic"));
            app.busy.set(0);
            match result {
                Ok((preferences, session)) => {
                    app.preferences.replace(preferences);
                    app.apply_appearance();
                    let changes = session.changes();
                    app.session.replace(Some(session));
                    app.set_ready(true);
                    app.refresh();
                    let weak = Rc::downgrade(&app);
                    glib::spawn_future_local(async move {
                        while changes.recv().await.is_ok() {
                            glib::timeout_future(Duration::from_millis(120)).await;
                            let Some(app) = weak.upgrade() else {
                                break;
                            };
                            if app.closing.get() {
                                break;
                            }
                            app.refresh();
                        }
                    });
                    app.pump_invitations();
                }
                Err(key) => app.startup_error(key),
            }
            drop(hold);
        });
    }

    fn startup_error(&self, key: &str) {
        self.object::<adw::StatusPage>("startup")
            .set_title(&text("linux_startup_error"));
        self.object::<adw::StatusPage>("startup")
            .set_description(Some(&text(key)));
        self.object::<gtk::Spinner>("spinner").set_visible(false);
        self.object::<gtk::Button>("retry").set_visible(true);
    }

    fn dispatch<T: Send + 'static>(
        self: &Rc<Self>,
        operation: impl FnOnce(Arc<Session>) -> Result<T> + Send + 'static,
        complete: impl FnOnce(Rc<Self>, Result<T>) + 'static,
    ) {
        let Some(session) = self.session.borrow().clone() else {
            return;
        };
        if self.closing.get() {
            return;
        }
        self.busy.set(self.busy.get() + 1);
        let app = self.clone();
        let hold = self.application.hold();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || operation(session))
                .await
                .unwrap_or(Err("error_generic"));
            app.busy.set(app.busy.get() - 1);
            if !app.closing.get() {
                complete(app, result);
            }
            drop(hold);
        });
    }

    fn refresh(self: &Rc<Self>) {
        if self.refreshing.replace(true) {
            self.refresh_again.set(true);
            return;
        }
        self.dispatch(
            |session| session.snapshot(),
            |app, result| {
                app.refreshing.set(false);
                match result {
                    Ok(snapshot) => {
                        app.snapshot.replace(Some(snapshot));
                        app.render();
                    }
                    Err(key) => app.error(key),
                }
                if app.refresh_again.replace(false) {
                    app.refresh();
                }
            },
        );
    }

    fn error(&self, key: &str) {
        #[cfg(test)]
        eprintln!("GTK feedback: {key}");
        self.toasts.add_toast(adw::Toast::new(&text(key)));
    }

    fn apply_appearance(&self) {
        self.application.style_manager().set_color_scheme(
            match self.preferences.borrow().theme.as_str() {
                "Light" => adw::ColorScheme::ForceLight,
                "Dark" => adw::ColorScheme::ForceDark,
                _ => adw::ColorScheme::Default,
            },
        );
    }

    fn request_close(self: &Rc<Self>) {
        if self.closing.get() || self.confirming_close.replace(true) {
            return;
        }
        let app = self.clone();
        glib::spawn_future_local(async move {
            let active = app.busy.get() != 0
                || app
                    .snapshot
                    .borrow()
                    .as_ref()
                    .is_some_and(Snapshot::has_obligations);
            if active
                && !dialogs::confirm(
                    &app.window,
                    "linux_close_title",
                    "linux_close_body",
                    "linux_quit_stop",
                )
                .await
            {
                app.confirming_close.set(false);
                return;
            }
            // Initialization must finish before its session can be shut down.
            while app.session.borrow().is_none() && app.busy.get() != 0 {
                glib::timeout_future(Duration::from_millis(30)).await;
            }
            while app.saving_preferences.get() {
                glib::timeout_future(Duration::from_millis(30)).await;
            }
            app.closing.set(true);
            app.set_ready(false);
            let session = app.session.borrow_mut().take();
            if let Some(session) = session {
                let _ = gio::spawn_blocking(move || session.close()).await;
            }
            app.window.destroy();
        });
    }
}
