use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferDirection {
    Send,
    Receive,
}

impl TransferDirection {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }
}

impl TryFrom<&str> for TransferDirection {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "send" => Ok(Self::Send),
            "receive" => Ok(Self::Receive),
            _ => bail!("unknown transfer direction: {value}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferStatus {
    Importing,
    Sharing,
    Receiving,
    Done,
    Failed,
    Cancelled,
    Stopped,
}

impl TransferStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Importing => "importing",
            Self::Sharing => "sharing",
            Self::Receiving => "receiving",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Stopped => "stopped",
        }
    }

    pub(crate) const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Importing,
                Self::Sharing | Self::Failed | Self::Cancelled
            ) | (Self::Sharing, Self::Stopped | Self::Failed)
                | (Self::Receiving, Self::Done | Self::Failed | Self::Cancelled)
        )
    }
}

impl TryFrom<&str> for TransferStatus {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "importing" => Ok(Self::Importing),
            "sharing" => Ok(Self::Sharing),
            "receiving" => Ok(Self::Receiving),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "stopped" => Ok(Self::Stopped),
            _ => bail!("unknown transfer status: {value}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReceiverRequestStatus {
    Requested,
    Accepted,
    Refused,
    Expired,
    Completed,
    Failed,
}

impl ReceiverRequestStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Accepted => "accepted",
            Self::Refused => "refused",
            Self::Expired => "expired",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for ReceiverRequestStatus {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "requested" => Ok(Self::Requested),
            "accepted" => Ok(Self::Accepted),
            "refused" => Ok(Self::Refused),
            "expired" => Ok(Self::Expired),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => bail!("unknown receiver request status: {value}"),
        }
    }
}
