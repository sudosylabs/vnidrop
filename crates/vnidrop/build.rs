use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let version_file = manifest_dir.join("../../version.properties");
    println!("cargo:rerun-if-changed={}", version_file.display());

    let contents = fs::read_to_string(&version_file)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", version_file.display()));
    let versions: Vec<_> = contents
        .lines()
        .filter_map(|line| line.strip_prefix("PRODUCT_VERSION="))
        .collect();
    assert_eq!(
        versions.len(),
        1,
        "{} must contain exactly one PRODUCT_VERSION",
        version_file.display()
    );
    let version = versions[0];
    let components: Vec<_> = version.split('.').collect();
    assert!(
        components.len() == 3
            && components.iter().all(|component| {
                component.parse::<u16>().is_ok()
                    && (component == &"0" || !component.starts_with('0'))
            }),
        "PRODUCT_VERSION must use canonical MAJOR.MINOR.PATCH integers"
    );
    println!("cargo:rustc-env=VNIDROP_PRODUCT_VERSION={version}");
}
