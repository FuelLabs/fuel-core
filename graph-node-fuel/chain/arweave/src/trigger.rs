use graph::blockchain::Block;
use graph::blockchain::MappingTriggerTrait;
use graph::blockchain::TriggerData;
use graph::cheap_clone::CheapClone;
use graph::prelude::web3::types::H256;
use graph::prelude::BlockNumber;
use graph::runtime::asc_new;
use graph::runtime::gas::GasCounter;
use graph::runtime::AscHeap;
use graph::runtime::AscPtr;
use graph::runtime::HostExportError;
use graph_runtime_wasm::module::ToAscPtr;
use std::{cmp::Ordering, sync::Arc};

use crate::codec;

// Logging the block is too verbose, so this strips the block from the trigger for Debug.
impl std::fmt::Debug for ArweaveTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[derive(Debug)]
        pub enum MappingTriggerWithoutBlock {
            Block,
            Transaction(Arc<codec::Transaction>),
        }

        let trigger_without_block = match self {
            ArweaveTrigger::Block(_) => MappingTriggerWithoutBlock::Block,
            ArweaveTrigger::Transaction(tx) => {
                MappingTriggerWithoutBlock::Transaction(tx.tx.clone())
            }
        };

        write!(f, "{:?}", trigger_without_block)
    }
}

impl ToAscPtr for ArweaveTrigger {
    fn to_asc_ptr<H: AscHeap>(
        self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<()>, HostExportError> {
        Ok(match self {
            ArweaveTrigger::Block(block) => asc_new(heap, block.as_ref(), gas)?.erase(),
            ArweaveTrigger::Transaction(tx) => asc_new(heap, tx.as_ref(), gas)?.erase(),
        })
    }
}

#[derive(Clone)]
pub enum ArweaveTrigger {
    Block(Arc<codec::Block>),
    Transaction(Arc<TransactionWithBlockPtr>),
}

impl CheapClone for ArweaveTrigger {
    fn cheap_clone(&self) -> ArweaveTrigger {
        match self {
            ArweaveTrigger::Block(block) => ArweaveTrigger::Block(block.cheap_clone()),
            ArweaveTrigger::Transaction(tx) => ArweaveTrigger::Transaction(tx.cheap_clone()),
        }
    }
}

impl PartialEq for ArweaveTrigger {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Block(a_ptr), Self::Block(b_ptr)) => a_ptr == b_ptr,
            (Self::Transaction(a_tx), Self::Transaction(b_tx)) => a_tx.tx.id == b_tx.tx.id,
            _ => false,
        }
    }
}

impl Eq for ArweaveTrigger {}

impl ArweaveTrigger {
    pub fn block_number(&self) -> BlockNumber {
        match self {
            ArweaveTrigger::Block(block) => block.number(),
            ArweaveTrigger::Transaction(tx) => tx.block.number(),
        }
    }

    pub fn block_hash(&self) -> H256 {
        match self {
            ArweaveTrigger::Block(block) => block.ptr().hash_as_h256(),
            ArweaveTrigger::Transaction(tx) => tx.block.ptr().hash_as_h256(),
        }
    }

    fn error_context(&self) -> std::string::String {
        match self {
            ArweaveTrigger::Block(..) => {
                format!("Block #{} ({})", self.block_number(), self.block_hash())
            }
            ArweaveTrigger::Transaction(tx) => {
                format!(
                    "Tx #{}, block #{}({})",
                    base64_url::encode(&tx.tx.id),
                    self.block_number(),
                    self.block_hash()
                )
            }
        }
    }
}

impl Ord for ArweaveTrigger {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Keep the order when comparing two block triggers
            (Self::Block(..), Self::Block(..)) => Ordering::Equal,

            // Block triggers always come last
            (Self::Block(..), _) => Ordering::Greater,
            (_, Self::Block(..)) => Ordering::Less,

            // Execution outcomes have no intrinsic ordering information so we keep the order in
            // which they are included in the `txs` field of `Block`.
            (Self::Transaction(..), Self::Transaction(..)) => Ordering::Equal,
        }
    }
}

impl PartialOrd for ArweaveTrigger {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TriggerData for ArweaveTrigger {
    fn error_context(&self) -> String {
        self.error_context()
    }

    fn address_match(&self) -> Option<&[u8]> {
        None
    }
}

impl MappingTriggerTrait for ArweaveTrigger {
    fn error_context(&self) -> String {
        self.error_context()
    }
}

pub struct TransactionWithBlockPtr {
    // REVIEW: Do we want to actually also have those two below behind an `Arc` wrapper?
    pub tx: Arc<codec::Transaction>,
    pub block: Arc<codec::Block>,
}
