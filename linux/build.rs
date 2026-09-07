use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=data/resources.xml");
    println!("cargo:rerun-if-changed=../assets/linux/app-icon.svg");
    println!("cargo:rerun-if-changed=../assets/linux/qr-code-symbolic.svg");
    if env::var_os("CARGO_FEATURE_GUI").is_none()
        || env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux")
    {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("icons.gresource");
    let status = Command::new("glib-compile-resources")
        .arg("data/resources.xml")
        .arg("--sourcedir=../assets/linux")
        .arg("--target")
        .arg(output)
        .status()
        .expect("install GLib development tools (glib-compile-resources)");
    assert!(
        status.success(),
        "could not compile application icon resources"
    );
}
