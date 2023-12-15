pub(crate) use crate::protobuf::pbcodec::*;

use graph::blockchain::Block as BlockchainBlock;
use graph::{
    blockchain::BlockPtr,
    prelude::{anyhow::anyhow, BlockNumber, Error},
};

use std::convert::TryFrom;

impl Block {
    pub fn header(&self) -> Result<&Header, Error> {
        self.header
            .as_ref()
            .ok_or_else(|| anyhow!("block data missing header field"))
    }

    pub fn begin_block_events(&self) -> Result<impl Iterator<Item = &Event>, Error> {
        let events = self
            .result_begin_block
            .as_ref()
            .ok_or_else(|| anyhow!("block data missing result_begin_block field"))?
            .events
            .iter();

        Ok(events)
    }

    pub fn end_block_events(&self) -> Result<impl Iterator<Item = &Event>, Error> {
        let events = self
            .result_end_block
            .as_ref()
            .ok_or_else(|| anyhow!("block data missing result_end_block field"))?
            .events
            .iter();

        Ok(events)
    }

    pub fn transactions(&self) -> impl Iterator<Item = &TxResult> {
        self.transactions.iter()
    }

    pub fn parent_ptr(&self) -> Result<Option<BlockPtr>, Error> {
        let header = self.header()?;

        Ok(header
            .last_block_id
            .as_ref()
            .map(|last_block_id| BlockPtr::from((last_block_id.hash.clone(), header.height - 1))))
    }
}

impl TryFrom<Block> for BlockPtr {
    type Error = Error;

    fn try_from(b: Block) -> Result<BlockPtr, Error> {
        BlockPtr::try_from(&b)
    }
}

impl<'a> TryFrom<&'a Block> for BlockPtr {
    type Error = Error;

    fn try_from(b: &'a Block) -> Result<BlockPtr, Error> {
        let header = b.header()?;
        Ok(BlockPtr::from((header.hash.clone(), header.height)))
    }
}

impl BlockchainBlock for Block {
    fn number(&self) -> i32 {
        BlockNumber::try_from(self.header().unwrap().height).unwrap()
    }

    fn ptr(&self) -> BlockPtr {
        BlockPtr::try_from(self).unwrap()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        self.parent_ptr().unwrap()
    }
}

impl HeaderOnlyBlock {
    pub fn header(&self) -> Result<&Header, Error> {
        self.header
            .as_ref()
            .ok_or_else(|| anyhow!("block data missing header field"))
    }

    pub fn parent_ptr(&self) -> Result<Option<BlockPtr>, Error> {
        let header = self.header()?;

        Ok(header
            .last_block_id
            .as_ref()
            .map(|last_block_id| BlockPtr::from((last_block_id.hash.clone(), header.height - 1))))
    }
}

impl From<&Block> for HeaderOnlyBlock {
    fn from(b: &Block) -> HeaderOnlyBlock {
        HeaderOnlyBlock {
            header: b.header.clone(),
        }
    }
}

impl TryFrom<HeaderOnlyBlock> for BlockPtr {
    type Error = Error;

    fn try_from(b: HeaderOnlyBlock) -> Result<BlockPtr, Error> {
        BlockPtr::try_from(&b)
    }
}

impl<'a> TryFrom<&'a HeaderOnlyBlock> for BlockPtr {
    type Error = Error;

    fn try_from(b: &'a HeaderOnlyBlock) -> Result<BlockPtr, Error> {
        let header = b.header()?;

        Ok(BlockPtr::from((header.hash.clone(), header.height)))
    }
}

impl BlockchainBlock for HeaderOnlyBlock {
    fn number(&self) -> i32 {
        BlockNumber::try_from(self.header().unwrap().height).unwrap()
    }

    fn ptr(&self) -> BlockPtr {
        BlockPtr::try_from(self).unwrap()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        self.parent_ptr().unwrap()
    }
}

impl EventData {
    pub fn event(&self) -> Result<&Event, Error> {
        self.event
            .as_ref()
            .ok_or_else(|| anyhow!("event data missing event field"))
    }
    pub fn block(&self) -> Result<&HeaderOnlyBlock, Error> {
        self.block
            .as_ref()
            .ok_or_else(|| anyhow!("event data missing block field"))
    }
}

impl TransactionData {
    pub fn tx_result(&self) -> Result<&TxResult, Error> {
        self.tx
            .as_ref()
            .ok_or_else(|| anyhow!("transaction data missing tx field"))
    }

    pub fn response_deliver_tx(&self) -> Result<&ResponseDeliverTx, Error> {
        self.tx_result()?
            .result
            .as_ref()
            .ok_or_else(|| anyhow!("transaction data missing result field"))
    }

    pub fn block(&self) -> Result<&HeaderOnlyBlock, Error> {
        self.block
            .as_ref()
            .ok_or_else(|| anyhow!("transaction data missing block field"))
    }
}

impl MessageData {
    pub fn message(&self) -> Result<&prost_types::Any, Error> {
        self.message
            .as_ref()
            .ok_or_else(|| anyhow!("message data missing message field"))
    }

    pub fn block(&self) -> Result<&HeaderOnlyBlock, Error> {
        self.block
            .as_ref()
            .ok_or_else(|| anyhow!("message data missing block field"))
    }
}
