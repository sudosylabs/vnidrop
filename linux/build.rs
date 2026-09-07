mod build_config;
use std::{env, path::PathBuf, process::Command};

fn main() {
    for name in [
        "VNIDROP_DIAGNOSTICS_ENDPOINT",
        "VNIDROP_DIAGNOSTICS_INGEST_KEY",
        "GRADLE_USER_HOME",
        "HOME",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let project = PathBuf::from("../gradle.properties");
    let gradle = env::var_os("GRADLE_USER_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".gradle")));
    let user = gradle.map(|path| path.join("gradle.properties"));
    println!("cargo:rerun-if-changed={}", project.display());
    if let Some(path) = &user {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let (endpoint, key) = build_config::diagnostics(
        &std::fs::read_to_string(project).unwrap_or_default(),
        &user
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default(),
        env::var("VNIDROP_DIAGNOSTICS_ENDPOINT").ok(),
        env::var("VNIDROP_DIAGNOSTICS_INGEST_KEY").ok(),
    );
    // Keep the embedded ingest key out of Cargo's build-script output.
    std::fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("diagnostics_config.rs"),
        format!("const BUILD_ENDPOINT: &str = {endpoint:?};\nconst BUILD_KEY: &str = {key:?};\n"),
    )
    .expect("write diagnostics build configuration");
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
