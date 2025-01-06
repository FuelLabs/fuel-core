use std::{
    iter::Map,
    ops::{
        Deref,
        RangeInclusive,
    },
};

use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Wei(u128);

impl Deref for Wei {
    type Target = u128;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GasUnits(pub u64);

impl Deref for GasUnits {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BytesSize(pub u64);

impl Deref for BytesSize {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockCommitterCosts {
    /// The cost of the block, supposedly in Wei but need to check
    pub cost: Wei,
    pub size: BytesSize,
    pub da_block_height: DaBlockHeight,
    pub start_height: BlockHeight,
    pub end_height: BlockHeight,
}

impl BlockCommitterCosts {
    pub fn iter(&self) -> impl Iterator<Item = BlockHeight> {
        let start_height = *self.start_height;
        let end_height = *self.end_height;
        (start_height..=end_height)
            .map(|raw_block_height| BlockHeight::from(raw_block_height))
    }

    pub fn len(&self) -> u32 {
        // Remove 1 from end height because range is inclusive.
        self.end_height
            .saturating_sub(1)
            .saturating_sub(*self.start_height)
    }
}

impl IntoIterator for BlockCommitterCosts {
    type Item = BlockHeight;

    // `impl Trait` in associated types is unstable
    // see issue #63063 <https://github.com/rust-lang/rust/issues/63063> for more information
    // We must specify the concrete type here.
    type IntoIter = Map<RangeInclusive<u32>, fn(u32) -> BlockHeight>;

    fn into_iter(self) -> Self::IntoIter {
        let start_height = *self.start_height;
        let end_height = *self.end_height;
        (start_height..=end_height)
            .map(|raw_block_height| BlockHeight::from(raw_block_height))
    }
}

pub struct Layer2BlockData {
    pub block_height: BlockHeight,
    pub block_size: BytesSize,
    pub gas_consumed: GasUnits,
    pub capacity: GasUnits,
    pub bytes_capacity: BytesSize,
    pub transactions_count: usize,
}
