#[cfg(target_os = "linux")]
mod app;

#[cfg(target_os = "linux")]
fn main() -> gtk::glib::ExitCode {
    app::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The GNOME host runs on Linux. See linux/README.md.");
    std::process::exit(1);
}
