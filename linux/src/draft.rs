use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use vnidrop::{ShareMetadataInput, ShareResult, ShareSource, SourceKind};

use crate::{error::Result, session::Session};

pub fn sources(paths: Vec<PathBuf>) -> Result<Vec<ShareSource>> {
    if paths.is_empty() {
        return Err("error_share_empty");
    }
    let mut result = Vec::new();
    for path in paths {
        let metadata = path.metadata().map_err(|_| "error_filesystem")?;
        if !metadata.is_file() && !metadata.is_dir() {
            return Err("linux_local_files_only");
        }
        let value = path.to_str().ok_or("linux_local_files_only")?.to_owned();
        if result
            .iter()
            .any(|source: &ShareSource| source.value == value)
        {
            continue;
        }
        result.push(ShareSource {
            kind: SourceKind::Path,
            value,
            display_name: Some(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("linux_local_files_only")?
                    .to_owned(),
            ),
            is_directory: metadata.is_dir(),
        });
    }
    if result.len() > 1 && result.iter().any(|source| source.is_directory) {
        return Err("linux_invalid_selection");
    }
    Ok(result)
}

/// Cancellation intent survives the interval before the core registers import.
pub struct Preparation {
    pub id: u64,
    cancelled: AtomicBool,
    finished: AtomicBool,
}

impl Preparation {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            id: (uuid::Uuid::new_v4().as_u128() as u64) & i64::MAX as u64,
            cancelled: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        })
    }

    pub fn run(
        &self,
        session: &Session,
        sources: Vec<ShareSource>,
        mut metadata: ShareMetadataInput,
    ) -> Result<ShareResult> {
        let _finished = Finished(&self.finished);
        if self.cancelled.load(Ordering::Acquire) {
            return Err("progress_cancelled");
        }
        metadata.transfer_id = self.id;
        let result = session.call(|core| core.share_files(sources, metadata));
        if self.cancelled.load(Ordering::Acquire) {
            let _ = session.call(|core| core.cancel_transfer(self.id));
            return Err("progress_cancelled");
        }
        result
    }

    pub fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self, session: &Session) {
        self.request_cancel();
        loop {
            let result = session.call(|core| core.cancel_transfer(self.id));
            if self.finished.load(Ordering::Acquire) || result == Err("linux_session_closed") {
                break;
            }
            // A call before core registration cannot cancel an import that has
            // not started yet. Keep intent alive through registration/publication.
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

struct Finished<'a>(&'a AtomicBool);

impl Drop for Finished<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_accepts_files_or_one_folder_and_rejects_mixed_drafts() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("résumé.txt");
        std::fs::write(&file, b"hello").unwrap();
        let selected = sources(vec![file.clone(), file.clone()]).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].display_name.as_deref(), Some("résumé.txt"));
        assert!(sources(vec![dir.path().into()]).unwrap()[0].is_directory);
        assert_eq!(
            sources(vec![file, dir.path().into()]).unwrap_err(),
            "linux_invalid_selection"
        );
    }
}
