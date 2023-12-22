use crate::codec;
use graph::{
    blockchain::{
        Block,
        MappingTriggerTrait,
        TriggerData,
    },
    prelude::{
        web3::types::H256,
        BlockNumber,
        CheapClone,
    },
};
use std::{
    cmp::Ordering,
    sync::Arc,
};

#[derive(Debug, Clone)]
pub enum FuelTrigger {
    Block(Arc<codec::Block>),
    // Receipt(Arc<ReceiptWithOutcome>),
}

impl FuelTrigger {
    pub fn block_number(&self) -> BlockNumber {
        match self {
            FuelTrigger::Block(block) => block.number(),
        }
    }

    pub fn block_hash(&self) -> H256 {
        match self {
            FuelTrigger::Block(block) => block.ptr().hash_as_h256(),
        }
    }

    fn error_context(&self) -> std::string::String {
        match self {
            FuelTrigger::Block(..) => {
                format!("Block #{} ({})", self.block_number(), self.block_hash())
            }
        }
    }
}

// Todo Emir
impl CheapClone for FuelTrigger {
    fn cheap_clone(&self) -> FuelTrigger {
        match self {
            FuelTrigger::Block(block) => FuelTrigger::Block(block.cheap_clone()),
        }
    }
}

impl TriggerData for FuelTrigger {
    fn error_context(&self) -> String {
        self.error_context()
    }

    fn address_match(&self) -> Option<&[u8]> {
        None
    }
}

impl PartialEq for FuelTrigger {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Block(a_ptr), Self::Block(b_ptr)) => a_ptr == b_ptr,
        }
    }
}

impl PartialOrd for FuelTrigger {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for FuelTrigger {}

impl Ord for FuelTrigger {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Keep the order when comparing two block triggers
            (Self::Block(..), Self::Block(..)) => Ordering::Equal,

            // Block triggers always come last
            (Self::Block(..), _) => Ordering::Greater,
            (_, Self::Block(..)) => Ordering::Less,
        }
    }
}

impl MappingTriggerTrait for FuelTrigger {
    fn error_context(&self) -> String {
        self.error_context()
    }
}
