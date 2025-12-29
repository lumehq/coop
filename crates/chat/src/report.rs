use std::fmt::Display;

use gpui::SharedString;
use nostr_sdk::prelude::*;

/// Send error
#[derive(Debug, Clone)]
pub enum SendError {
    RelayNotFound,
    EncryptionNotFound,
    Custom(String),
}

impl Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::RelayNotFound => write!(f, "Messaging Relay not found"),
            SendError::EncryptionNotFound => write!(f, "Encryption Key not found"),
            SendError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<&SendError> for SharedString {
    fn from(val: &SendError) -> Self {
        SharedString::new(val.to_string())
    }
}

/// Send report
#[derive(Debug, Clone)]
pub struct SendReport {
    /// Receiver's public key.
    pub receiver: PublicKey,

    /// Message on hold for resending later.
    pub on_hold: Option<Event>,

    /// Status of the send operation.
    pub status: Option<Output<EventId>>,

    /// Error message for the send operation.
    pub error: Option<SendError>,
}

impl SendReport {
    pub fn new(receiver: PublicKey) -> Self {
        Self {
            receiver,
            on_hold: None,
            status: None,
            error: None,
        }
    }

    /// Set the message on hold for resending later.
    pub fn on_hold(mut self, event: Event) -> Self {
        self.on_hold = Some(event);
        self
    }

    /// Set the status of the send operation.
    pub fn status(mut self, output: Output<EventId>) -> Self {
        self.status = Some(output);
        self
    }

    /// Set the error message for the send operation.
    pub fn error(mut self, error: SendError) -> Self {
        self.error = Some(error);
        self
    }

    /// Check if the send operation was successful.
    pub fn is_sent_success(&self) -> bool {
        if let Some(output) = self.status.as_ref() {
            !output.success.is_empty()
        } else {
            false
        }
    }
}
