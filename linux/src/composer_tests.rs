use super::*;
use vnidrop::SourceKind;

fn file(name: &str) -> ShareSource {
    ShareSource {
        kind: SourceKind::Path,
        value: format!("/tmp/{name}"),
        display_name: Some(name.into()),
        is_directory: false,
    }
}

fn name(count: usize) -> String {
    format!("{count} fichiers")
}

fn select(draft: &mut TransferDraft, mode: SelectionMode, files: Vec<ShareSource>) {
    let request = draft.begin_pick(mode).unwrap();
    draft.complete_pick(request, Ok(files), name).unwrap();
}

#[test]
fn editing_selection_tracks_automatic_names_and_preserves_manual_names() {
    let mut draft = TransferDraft::new("Sender".into());
    assert!(!draft.can_submit());
    select(
        &mut draft,
        SelectionMode::Replace,
        vec![file("a"), file("b")],
    );
    assert_eq!(draft.transfer_name(), "2 fichiers");
    let first = draft.sources()[0].id;
    draft.remove_source(first, name);
    assert_eq!(draft.transfer_name(), "b");
    draft.change_transfer_name("Documents".into());
    select(&mut draft, SelectionMode::Add, vec![file("c"), file("b")]);
    assert_eq!(draft.sources().len(), 2);
    assert_eq!(draft.transfer_name(), "Documents");
    let next = draft.sources()[0].id;
    draft.remove_source(next, name);
    assert_eq!(draft.transfer_name(), "Documents");
    select(&mut draft, SelectionMode::Replace, vec![file("new")]);
    assert_eq!(draft.transfer_name(), "new");
    assert_ne!(draft.sources()[0].id, first);
    draft.clear_sources();
    assert_eq!(draft.transfer_name(), "");
    assert!(!draft.can_submit());
}

#[test]
fn cancelled_failed_and_invalid_pickers_preserve_the_valid_draft() {
    let mut draft = TransferDraft::new("Sender".into());
    select(&mut draft, SelectionMode::Replace, vec![file("original")]);
    draft.change_transfer_name("Keep this".into());
    let id = draft.sources()[0].id;
    let request = draft.begin_pick(SelectionMode::Replace).unwrap();
    assert!(!draft.can_submit());
    assert!(draft.begin_pick(SelectionMode::Add).is_none());
    draft.complete_pick(request, Ok(vec![]), name).unwrap();
    let request = draft.begin_pick(SelectionMode::Replace).unwrap();
    assert_eq!(
        draft.complete_pick(request, Err("error_filesystem"), name),
        Err("error_filesystem")
    );
    let request = draft.begin_pick(SelectionMode::Add).unwrap();
    let mut folder = file("folder");
    folder.is_directory = true;
    assert_eq!(
        draft.complete_pick(request, Ok(vec![folder.clone()]), name),
        Err("linux_invalid_selection")
    );
    assert_eq!(draft.sources()[0].id, id);
    assert_eq!(draft.transfer_name(), "Keep this");
    assert!(draft.can_submit());
    select(&mut draft, SelectionMode::Replace, vec![folder]);
    assert!(draft.sources()[0].source.is_directory);
    assert_eq!(draft.transfer_name(), "folder");
}

#[test]
fn stale_picker_completion_cannot_replace_new_selection_or_reopen_dismissed_draft() {
    let mut draft = TransferDraft::new("Sender".into());
    let old = draft.begin_pick(SelectionMode::Replace).unwrap();
    draft
        .complete_pick(old, Ok(vec![file("first")]), name)
        .unwrap();
    let current = draft.begin_pick(SelectionMode::Replace).unwrap();
    draft
        .complete_pick(old, Ok(vec![file("stale")]), name)
        .unwrap();
    assert_eq!(
        draft.sources()[0].source.display_name.as_deref(),
        Some("first")
    );
    assert!(!draft.editable());
    assert!(draft.dismiss());
    draft
        .complete_pick(current, Ok(vec![file("late")]), name)
        .unwrap();
    assert!(draft.sources().is_empty());
    assert!(draft.begin_submission().is_none());
    assert!(draft.begin_pick(SelectionMode::Replace).is_none());
}

#[test]
fn submission_is_single_flight_and_failure_or_cancellation_preserves_retry_input() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("original.txt");
    std::fs::write(&path, b"original bytes").unwrap();
    let mut draft = TransferDraft::new(" Sender ".into());
    select(
        &mut draft,
        SelectionMode::Replace,
        crate::draft::sources(vec![path.clone()]).unwrap(),
    );
    draft.change_transfer_name("   ".into());
    assert!(draft.begin_submission().is_none());
    draft.change_transfer_name(" Documents ".into());
    draft.change_access_mode(TransferAccessMode::Public);
    let submission = draft.begin_submission().unwrap();
    assert_eq!(submission.transfer_name, "Documents");
    assert_eq!(submission.sender_name, "Sender");
    assert_eq!(submission.access_mode, TransferAccessMode::Public);
    assert_eq!(submission.sources[0].value, path.to_str().unwrap());
    assert!(draft.begin_submission().is_none());
    assert!(!draft.dismiss());
    draft.clear_sources();
    draft.change_transfer_name("ignored".into());
    draft.change_sender_name("ignored".into());
    draft.change_access_mode(TransferAccessMode::ApprovalRequired);
    draft.finish_submission(false);
    let retry = draft.begin_submission().unwrap();
    assert_eq!(retry.transfer_name, submission.transfer_name);
    assert_eq!(retry.sender_name, submission.sender_name);
    assert_eq!(retry.access_mode, submission.access_mode);
    draft.finish_submission(true);
    assert!(draft.sources().is_empty());
    assert!(draft.begin_submission().is_none());
    assert_eq!(std::fs::read(path).unwrap(), b"original bytes");
}
