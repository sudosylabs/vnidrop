use std::{
    pin::Pin,
    time::{Duration, Instant},
};

use anyhow::Result;
use futures_lite::{Stream, StreamExt as _};
use iroh_blobs::api::proto::ExportRangesItem;
use serde_json::json;

use super::CoreInner;

const BATCH_BYTES: usize = 1024 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub(super) struct BufferedExport<'a, S> {
    stream: Pin<Box<S>>,
    path: &'a str,
    expected_size: u64,
    file_size: Option<u64>,
    exported: u64,
    last_progress: Option<Instant>,
}

impl<'a, S: Stream<Item = ExportRangesItem>> BufferedExport<'a, S> {
    pub(super) fn new(stream: S, path: &'a str, expected_size: u64) -> Self {
        Self {
            stream: Box::pin(stream),
            path,
            expected_size,
            file_size: None,
            exported: 0,
            last_progress: None,
        }
    }

    pub(super) async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        let path = self.path;
        let size = match self.file_size {
            Some(size) => size,
            None => match self.stream.as_mut().next().await {
                Some(ExportRangesItem::Size(size)) => {
                    anyhow::ensure!(
                        size == self.expected_size,
                        "export size for {path} differs from its collection size"
                    );
                    self.file_size = Some(size);
                    size
                }
                Some(ExportRangesItem::Error(error)) => {
                    anyhow::bail!("export failed for {path}: {error}");
                }
                _ => anyhow::bail!("export stream for {path} did not report its size"),
            },
        };
        let remaining = size - self.exported;
        if remaining == 0 {
            return Ok(None);
        }
        let mut chunk = Vec::with_capacity(remaining.min(BATCH_BYTES as u64) as usize);
        while chunk.len() < BATCH_BYTES && self.exported < size {
            match self.stream.as_mut().next().await {
                Some(ExportRangesItem::Data(leaf)) => {
                    anyhow::ensure!(
                        leaf.offset == self.exported
                            && (leaf.data.len() as u64) <= size - self.exported,
                        "export stream for {path} yielded out-of-range data"
                    );
                    self.exported += leaf.data.len() as u64;
                    chunk.extend_from_slice(&leaf.data);
                }
                Some(ExportRangesItem::Error(error)) => {
                    anyhow::bail!("export failed for {path}: {error}");
                }
                _ => anyhow::bail!("export stream for {path} ended before its reported size"),
            }
        }
        Ok(Some(chunk))
    }

    pub(super) fn finish(self) -> Result<u64> {
        anyhow::ensure!(
            self.file_size == Some(self.expected_size) && self.exported == self.expected_size,
            "export for {} wrote {} bytes, expected {}",
            self.path,
            self.exported,
            self.expected_size
        );
        Ok(self.exported)
    }

    pub(super) fn report_progress(
        &mut self,
        core: &CoreInner,
        transfer_id: u64,
        total_files: u64,
        current_file_index: u64,
    ) {
        if self.file_size != Some(self.exported)
            && self
                .last_progress
                .is_some_and(|previous| previous.elapsed() < PROGRESS_INTERVAL)
        {
            return;
        }
        self.last_progress = Some(Instant::now());
        core.emit_transfer(
            transfer_id,
            "receive",
            "export",
            "progress",
            json!({
                "total_files": total_files,
                "current_file_index": current_file_index,
                "file_name": self.path,
                "file_size": self.file_size,
                "exported": self.exported,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn export_uses_the_downloaded_collection_size_before_it_can_finish() {
        let stream = futures_lite::stream::iter([ExportRangesItem::Size(0)]);
        let mut mismatched = BufferedExport::new(stream, "file", 1);
        assert!(mismatched.next_chunk().await.is_err());
        assert!(mismatched.finish().is_err());

        let stream = futures_lite::stream::iter([ExportRangesItem::Size(0)]);
        let mut empty = BufferedExport::new(stream, "empty", 0);
        assert!(empty.next_chunk().await.unwrap().is_none());
        assert_eq!(empty.finish().unwrap(), 0);
    }
}
