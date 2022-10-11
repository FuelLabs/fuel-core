use anyhow::Result;

use crate::model::BlockHeight;

pub trait BlockHeightDb: Send + Sync {
    fn block_height(&self) -> Result<BlockHeight>;
}
