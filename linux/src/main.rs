#[cfg(target_os = "linux")]
mod app;

#[cfg(target_os = "linux")]
fn main() -> gtk::glib::ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--check-diagnostics") {
        return if vnidrop_gnome::diagnostics::build_configured() {
            println!("Bug reporting is configured in this build.");
            gtk::glib::ExitCode::SUCCESS
        } else {
            eprintln!("Bug reporting is not configured in this build.");
            gtk::glib::ExitCode::FAILURE
        };
    }
    app::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The GNOME host runs on Linux. See linux/README.md.");
    std::process::exit(1);
}
