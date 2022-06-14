use anyhow::Result;
use fuel_core_interfaces::model::{BlockHeight, FuelBlock};
use std::borrow::Cow;

pub trait BlockProducerDatabase: Sync + Send {
    /// fetch previously committed block at given height
    fn get_block(&self, fuel_height: BlockHeight) -> Result<Cow<FuelBlock>>;
}
