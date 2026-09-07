use vnidrop::{ShareSource, TransferAccessMode};

use crate::error::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Add,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Editing,
    Picking { request: u64, mode: SelectionMode },
    Submitting,
    Closed,
}

#[derive(Clone, Debug)]
pub struct DraftSource {
    pub id: u64,
    pub source: ShareSource,
}

pub struct Submission {
    pub sources: Vec<ShareSource>,
    pub transfer_name: String,
    pub sender_name: String,
    pub access_mode: TransferAccessMode,
}

/// Owns one composition session, including stale picker results and name provenance.
pub struct TransferDraft {
    sources: Vec<DraftSource>,
    transfer_name: String,
    sender_name: String,
    access_mode: TransferAccessMode,
    automatic_name: bool,
    phase: Phase,
    next_request: u64,
    next_source: u64,
}

impl TransferDraft {
    pub fn new(sender_name: String) -> Self {
        Self {
            sources: Vec::new(),
            transfer_name: String::new(),
            sender_name,
            access_mode: TransferAccessMode::ApprovalRequired,
            automatic_name: true,
            phase: Phase::Editing,
            next_request: 1,
            next_source: 1,
        }
    }

    pub fn sources(&self) -> &[DraftSource] {
        &self.sources
    }

    pub fn transfer_name(&self) -> &str {
        &self.transfer_name
    }

    pub fn sender_name(&self) -> &str {
        &self.sender_name
    }

    pub fn access_mode(&self) -> TransferAccessMode {
        self.access_mode.clone()
    }

    pub fn editable(&self) -> bool {
        self.phase == Phase::Editing
    }

    pub fn is_submitting(&self) -> bool {
        self.phase == Phase::Submitting
    }

    pub fn can_submit(&self) -> bool {
        self.editable() && !self.sources.is_empty() && !self.transfer_name.trim().is_empty()
    }

    pub fn change_transfer_name(&mut self, value: String) {
        if self.editable() {
            self.automatic_name = false;
            self.transfer_name = value;
        }
    }

    pub fn change_sender_name(&mut self, value: String) {
        if self.editable() {
            self.sender_name = value;
        }
    }

    pub fn change_access_mode(&mut self, value: TransferAccessMode) {
        if self.editable() {
            self.access_mode = value;
        }
    }

    pub fn begin_pick(&mut self, mode: SelectionMode) -> Option<u64> {
        if !self.editable() {
            return None;
        }
        let request = self.next_request;
        self.next_request += 1;
        self.phase = Phase::Picking { request, mode };
        Some(request)
    }

    pub fn complete_pick(
        &mut self,
        request: u64,
        result: Result<Vec<ShareSource>>,
        multiple_files_name: impl Fn(usize) -> String,
    ) -> Result<()> {
        let Phase::Picking {
            request: current,
            mode,
        } = self.phase
        else {
            return Ok(());
        };
        if request != current {
            return Ok(());
        }
        self.phase = Phase::Editing;
        let sources = result?;
        if sources.is_empty() {
            return Ok(());
        }
        let mut selected = match mode {
            SelectionMode::Replace => Vec::new(),
            SelectionMode::Add => self.sources.clone(),
        };
        for source in sources {
            if selected
                .iter()
                .any(|item| item.source.value == source.value)
            {
                continue;
            }
            selected.push(DraftSource {
                id: self.next_source,
                source,
            });
            self.next_source += 1;
        }
        if selected.len() > 1 && selected.iter().any(|item| item.source.is_directory) {
            return Err("linux_invalid_selection");
        }
        self.sources = selected;
        if mode == SelectionMode::Replace {
            self.automatic_name = true;
        }
        self.update_name(multiple_files_name);
        Ok(())
    }

    pub fn remove_source(&mut self, id: u64, multiple_files_name: impl Fn(usize) -> String) {
        if !self.editable() {
            return;
        }
        self.sources.retain(|item| item.id != id);
        self.update_name(multiple_files_name);
    }

    pub fn clear_sources(&mut self) {
        if self.editable() {
            self.sources.clear();
            self.automatic_name = true;
            self.transfer_name.clear();
        }
    }

    pub fn begin_submission(&mut self) -> Option<Submission> {
        if !self.can_submit() {
            return None;
        }
        self.phase = Phase::Submitting;
        Some(Submission {
            sources: self
                .sources
                .iter()
                .map(|item| item.source.clone())
                .collect(),
            transfer_name: self.transfer_name.trim().into(),
            sender_name: self.sender_name.trim().into(),
            access_mode: self.access_mode.clone(),
        })
    }

    pub fn finish_submission(&mut self, succeeded: bool) {
        if self.phase == Phase::Submitting {
            self.phase = Phase::Editing;
            if succeeded {
                self.dismiss();
            }
        }
    }

    pub fn dismiss(&mut self) -> bool {
        if self.is_submitting() {
            return false;
        }
        self.phase = Phase::Closed;
        self.sources.clear();
        true
    }

    fn update_name(&mut self, multiple_files_name: impl Fn(usize) -> String) {
        if self.automatic_name {
            self.transfer_name = match self.sources.as_slice() {
                [] => String::new(),
                [item] => item.source.display_name.clone().unwrap_or_default(),
                items => multiple_files_name(items.len()),
            };
        }
    }
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;
