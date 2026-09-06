use std::path::Path;
#[cfg(unix)]
use std::{io::Read, os::fd::AsRawFd};

use crate::{
    api::{CoreLimits, ShareSource, SourceKind},
    filesystem::{
        cleanup_stale_temporary_files, collect_import_files, collect_import_files_with_limits,
        default_collection_name, path_to_string, percent_decode_file_url_path,
        validated_relative_string, AtomicOutputFile,
    },
};

#[test]
fn path_validation_rejects_unsafe_paths() {
    assert!(path_to_string(Path::new("../escape"), true).is_err());
    assert!(path_to_string(Path::new("/absolute"), true).is_err());
    #[cfg(unix)]
    assert!(validated_relative_string("bad\\name").is_err());
    assert!(validated_relative_string("").is_err());
}

#[cfg(windows)]
#[test]
fn windows_relative_paths_normalize_separators_and_reject_roots() {
    assert_eq!(
        validated_relative_string("folder\\file.txt").unwrap(),
        "folder/file.txt"
    );
    for path in [
        "C:\\file.txt",
        "C:file.txt",
        "\\\\server\\share\\file.txt",
        "folder\\..\\file.txt",
    ] {
        assert!(validated_relative_string(path).is_err(), "accepted {path}");
    }
}

#[test]
fn file_url_decodes_spaces() {
    assert_eq!(
        percent_decode_file_url_path("/tmp/My%20File.txt").unwrap(),
        "/tmp/My File.txt"
    );
}

#[cfg(unix)]
#[test]
fn file_descriptor_source_duplicates_and_streams() {
    let mut temp = tempfile::tempfile().unwrap();
    std::io::Write::write_all(&mut temp, b"fd-backed import").unwrap();
    std::io::Seek::rewind(&mut temp).unwrap();

    let files = collect_import_files(vec![ShareSource {
        kind: SourceKind::FileDescriptor,
        value: temp.as_raw_fd().to_string(),
        display_name: Some("from-fd.txt".to_string()),
        is_directory: false,
    }])
    .unwrap();

    let mut imported = files.into_iter().next().unwrap().source.open().unwrap();
    let mut content = String::new();
    imported.read_to_string(&mut content).unwrap();
    assert_eq!(content, "fd-backed import");
}

#[cfg(unix)]
#[test]
fn file_descriptor_source_rejects_invalid_values() {
    for value in ["not-an-fd", "-1"] {
        assert!(collect_import_files(vec![ShareSource {
            kind: SourceKind::FileDescriptor,
            value: value.to_string(),
            display_name: Some("from-fd.txt".to_string()),
            is_directory: false,
        }])
        .is_err());
    }
}

#[cfg(unix)]
#[test]
fn file_descriptor_source_rejects_directory_fds() {
    let temp = tempfile::tempdir().unwrap();
    let dir = std::fs::File::open(temp.path()).unwrap();
    use std::os::fd::AsRawFd;
    let error = collect_import_files(vec![ShareSource {
        kind: SourceKind::FileDescriptor,
        value: dir.as_raw_fd().to_string(),
        display_name: Some("folder".to_string()),
        is_directory: false,
    }])
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("regular file"),
        "directory fd must be rejected: {error}"
    );
}

#[test]
fn android_content_uri_must_be_opened_by_platform_code() {
    let error = collect_import_files(vec![ShareSource {
        kind: SourceKind::AndroidContentUri,
        value: "content://media/item".to_string(),
        display_name: Some("from-uri.txt".to_string()),
        is_directory: false,
    }])
    .unwrap_err()
    .to_string();
    assert!(error.contains("ParcelFileDescriptor"));
}

#[test]
fn directory_sources_preserve_safe_relative_names() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("picked");
    std::fs::create_dir_all(root.join("nested")).unwrap();
    std::fs::write(root.join("nested").join("a.txt"), b"a").unwrap();
    std::fs::write(root.join("b.txt"), b"b").unwrap();

    let mut files = collect_import_files(vec![ShareSource {
        kind: SourceKind::Path,
        value: root.to_string_lossy().to_string(),
        display_name: Some("Album".to_string()),
        is_directory: true,
    }])
    .unwrap();
    files.sort_by(|a, b| a.collection_name.cmp(&b.collection_name));

    assert_eq!(default_collection_name(&files), "Album");
    assert_eq!(files[0].collection_name, "Album/b.txt");
    assert_eq!(files[1].collection_name, "Album/nested/a.txt");
}

#[test]
fn atomic_output_commits_without_overwriting() {
    let output = tempfile::tempdir().unwrap();
    let (pending, mut file) = AtomicOutputFile::create(output.path(), "nested/file.txt").unwrap();
    std::io::Write::write_all(&mut file, b"complete").unwrap();
    file.sync_all().unwrap();
    drop(file);
    pending.commit().unwrap();

    assert_eq!(
        std::fs::read(output.path().join("nested/file.txt")).unwrap(),
        b"complete"
    );
    assert!(AtomicOutputFile::create(output.path(), "nested/file.txt").is_err());
    assert_eq!(
        std::fs::read(output.path().join("nested/file.txt")).unwrap(),
        b"complete"
    );
}

#[test]
fn atomic_output_commit_does_not_clobber_existing_peer() {
    let output = tempfile::tempdir().unwrap();
    let target = output.path().join("peer.txt");
    std::fs::write(&target, b"original").unwrap();

    let (pending, mut file) = AtomicOutputFile::create(output.path(), "other.txt").unwrap();
    std::io::Write::write_all(&mut file, b"new").unwrap();
    drop(file);
    pending.commit().unwrap();

    // Existing peer must stay intact while a different file commits.
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert_eq!(
        std::fs::read(output.path().join("other.txt")).unwrap(),
        b"new"
    );
}

#[test]
fn dropped_atomic_output_removes_partial_file() {
    let output = tempfile::tempdir().unwrap();
    let (pending, mut file) = AtomicOutputFile::create(output.path(), "partial.txt").unwrap();
    std::io::Write::write_all(&mut file, b"partial").unwrap();
    drop(file);
    drop(pending);

    assert!(!output.path().join("partial.txt").exists());
    assert!(std::fs::read_dir(output.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains(".part")));
}

#[cfg(unix)]
#[test]
fn atomic_output_rejects_symlinked_parent() {
    use std::os::unix::fs::symlink;

    let output = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), output.path().join("link")).unwrap();

    assert!(AtomicOutputFile::create(output.path(), "link/escape.txt").is_err());
    assert!(!outside.path().join("escape.txt").exists());
}

#[cfg(unix)]
#[test]
fn atomic_output_never_replaces_symlink_destination() {
    use std::os::unix::fs::symlink;

    let output = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), b"outside").unwrap();
    symlink(outside.path(), output.path().join("target.txt")).unwrap();

    assert!(AtomicOutputFile::create(output.path(), "target.txt").is_err());
    assert_eq!(std::fs::read(outside.path()).unwrap(), b"outside");
}

#[test]
fn cleanup_removes_only_vnidrop_temporary_files() {
    let output = tempfile::tempdir().unwrap();
    std::fs::write(output.path().join(".file.vnidrop-old.part"), b"partial").unwrap();
    std::fs::write(output.path().join("keep.part"), b"keep").unwrap();

    assert_eq!(
        cleanup_stale_temporary_files(output.path(), std::time::Duration::ZERO).unwrap(),
        1
    );
    assert!(!output.path().join(".file.vnidrop-old.part").exists());
    assert!(output.path().join("keep.part").exists());
}

#[test]
fn import_collection_limits_are_enforced_before_streaming() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("one.txt"), b"one").unwrap();
    std::fs::write(temp.path().join("two.txt"), b"two").unwrap();
    let source = ShareSource {
        kind: SourceKind::Path,
        value: temp.path().to_string_lossy().to_string(),
        display_name: Some("folder".to_string()),
        is_directory: true,
    };

    let file_limits = CoreLimits {
        max_collection_files: 1,
        ..CoreLimits::default()
    };
    assert!(collect_import_files_with_limits(vec![source.clone()], &file_limits).is_err());

    let size_limits = CoreLimits {
        max_collection_files: 10,
        max_total_bytes: 5,
        ..CoreLimits::default()
    };
    assert!(collect_import_files_with_limits(vec![source], &size_limits).is_err());

    let path_limits = CoreLimits {
        max_path_bytes: 4,
        ..CoreLimits::default()
    };
    let file = temp.path().join("long-name.txt");
    std::fs::write(&file, b"x").unwrap();
    assert!(collect_import_files_with_limits(
        vec![ShareSource {
            kind: SourceKind::Path,
            value: file.to_string_lossy().to_string(),
            display_name: Some("long-name.txt".to_string()),
            is_directory: false,
        }],
        &path_limits,
    )
    .is_err());
}

#[test]
fn generated_relative_paths_never_accept_traversal_components() {
    for prefix in ["", "folder/", "a/b/"] {
        for traversal in ["..", "../escape", "..\\escape"] {
            let candidate = format!("{prefix}{traversal}");
            assert!(
                validated_relative_string(&candidate).is_err(),
                "{candidate}"
            );
        }
    }
    assert!(validated_relative_string(".").is_err());
    assert!(validated_relative_string("/absolute").is_err());
}
