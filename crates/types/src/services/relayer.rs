//! The module contains types related to the relayer service.

use crate::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
};

/// The event that may come from the relayer.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The message event which was sent to the bridge.
    Message(Message),
}

impl Event {
    /// Returns the da height when event happened.
    pub fn da_height(&self) -> DaBlockHeight {
        match self {
            Event::Message(message) => message.da_height(),
        }
    }
}

impl From<Message> for Event {
    fn from(message: Message) -> Self {
        Event::Message(message)
    }
}
