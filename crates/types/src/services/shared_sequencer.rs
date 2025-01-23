//! Module defines types for shared sequencer.

use crate::{
    blockchain::primitives::BlockId,
    fuel_types::BlockHeight,
};

/// The blob posted to the shared sequencer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SSBlob {
    /// The Fuel block height.
    pub block_height: BlockHeight,
    /// The block ID that corresponds to the block height.
    pub block_id: BlockId,
}

/// The blobs posted to the shared sequencer.
pub type SSBlobs = alloc::vec::Vec<SSBlob>;
