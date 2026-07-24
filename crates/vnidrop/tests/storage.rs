use vnidrop::clear_inactive_transfer_cache;

#[test]
fn inactive_transfer_cache_is_removed_and_reports_reclaimed_bytes() {
    let app_data = tempfile::tempdir().unwrap();
    let blobs = app_data.path().join("blobs");
    std::fs::create_dir_all(blobs.join("data")).unwrap();
    std::fs::write(blobs.join("data").join("payload"), vec![9u8; 4096]).unwrap();
    std::fs::write(blobs.join("blobs.db"), vec![3u8; 512]).unwrap();

    let reclaimed =
        clear_inactive_transfer_cache(app_data.path().to_string_lossy().into_owned()).unwrap();

    assert_eq!(reclaimed, 4608);
    assert!(!blobs.exists());
}

#[test]
fn inactive_transfer_cache_rejects_relative_app_data_paths() {
    assert!(clear_inactive_transfer_cache("relative/path".to_string()).is_err());
}

#[test]
fn inactive_transfer_cache_rejects_filesystem_roots() {
    assert!(clear_inactive_transfer_cache(std::path::MAIN_SEPARATOR_STR.to_string()).is_err());
}
