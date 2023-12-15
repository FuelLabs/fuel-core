use std::{cmp::Ordering, sync::Arc};

use graph::blockchain::{Block, BlockHash, MappingTriggerTrait, TriggerData};
use graph::cheap_clone::CheapClone;
use graph::prelude::{BlockNumber, Error};
use graph::runtime::HostExportError;
use graph::runtime::{asc_new, gas::GasCounter, AscHeap, AscPtr};
use graph_runtime_wasm::module::ToAscPtr;

use crate::codec;
use crate::data_source::EventOrigin;

// Logging the block is too verbose, so this strips the block from the trigger for Debug.
impl std::fmt::Debug for CosmosTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[derive(Debug)]
        pub enum MappingTriggerWithoutBlock<'e> {
            Block,
            Event {
                event_type: &'e str,
                origin: EventOrigin,
            },
            Transaction,
            Message,
        }

        let trigger_without_block = match self {
            CosmosTrigger::Block(_) => MappingTriggerWithoutBlock::Block,
            CosmosTrigger::Event { event_data, origin } => MappingTriggerWithoutBlock::Event {
                event_type: &event_data.event().map_err(|_| std::fmt::Error)?.event_type,
                origin: *origin,
            },
            CosmosTrigger::Transaction(_) => MappingTriggerWithoutBlock::Transaction,
            CosmosTrigger::Message(_) => MappingTriggerWithoutBlock::Message,
        };

        write!(f, "{:?}", trigger_without_block)
    }
}

impl ToAscPtr for CosmosTrigger {
    fn to_asc_ptr<H: AscHeap>(
        self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<()>, HostExportError> {
        Ok(match self {
            CosmosTrigger::Block(block) => asc_new(heap, block.as_ref(), gas)?.erase(),
            CosmosTrigger::Event { event_data, .. } => {
                asc_new(heap, event_data.as_ref(), gas)?.erase()
            }
            CosmosTrigger::Transaction(transaction_data) => {
                asc_new(heap, transaction_data.as_ref(), gas)?.erase()
            }
            CosmosTrigger::Message(message_data) => {
                asc_new(heap, message_data.as_ref(), gas)?.erase()
            }
        })
    }
}

#[derive(Clone)]
pub enum CosmosTrigger {
    Block(Arc<codec::Block>),
    Event {
        event_data: Arc<codec::EventData>,
        origin: EventOrigin,
    },
    Transaction(Arc<codec::TransactionData>),
    Message(Arc<codec::MessageData>),
}

impl CheapClone for CosmosTrigger {}

impl PartialEq for CosmosTrigger {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Block(a_ptr), Self::Block(b_ptr)) => a_ptr == b_ptr,
            (
                Self::Event {
                    event_data: a_event_data,
                    origin: a_origin,
                },
                Self::Event {
                    event_data: b_event_data,
                    origin: b_origin,
                },
            ) => {
                if let (Ok(a_event), Ok(b_event)) = (a_event_data.event(), b_event_data.event()) {
                    a_event.event_type == b_event.event_type && a_origin == b_origin
                } else {
                    false
                }
            }
            (Self::Transaction(a_ptr), Self::Transaction(b_ptr)) => a_ptr == b_ptr,
            (Self::Message(a_ptr), Self::Message(b_ptr)) => a_ptr == b_ptr,
            _ => false,
        }
    }
}

impl Eq for CosmosTrigger {}

impl CosmosTrigger {
    pub(crate) fn with_event(
        event: codec::Event,
        block: codec::HeaderOnlyBlock,
        tx_context: Option<codec::TransactionContext>,
        origin: EventOrigin,
    ) -> CosmosTrigger {
        CosmosTrigger::Event {
            event_data: Arc::new(codec::EventData {
                event: Some(event),
                block: Some(block),
                tx: tx_context,
            }),
            origin,
        }
    }

    pub(crate) fn with_transaction(
        tx_result: codec::TxResult,
        block: codec::HeaderOnlyBlock,
    ) -> CosmosTrigger {
        CosmosTrigger::Transaction(Arc::new(codec::TransactionData {
            tx: Some(tx_result),
            block: Some(block),
        }))
    }

    pub(crate) fn with_message(
        message: ::prost_types::Any,
        block: codec::HeaderOnlyBlock,
        tx_context: codec::TransactionContext,
    ) -> CosmosTrigger {
        CosmosTrigger::Message(Arc::new(codec::MessageData {
            message: Some(message),
            block: Some(block),
            tx: Some(tx_context),
        }))
    }

    pub fn block_number(&self) -> Result<BlockNumber, Error> {
        match self {
            CosmosTrigger::Block(block) => Ok(block.number()),
            CosmosTrigger::Event { event_data, .. } => event_data.block().map(|b| b.number()),
            CosmosTrigger::Transaction(transaction_data) => {
                transaction_data.block().map(|b| b.number())
            }
            CosmosTrigger::Message(message_data) => message_data.block().map(|b| b.number()),
        }
    }

    pub fn block_hash(&self) -> Result<BlockHash, Error> {
        match self {
            CosmosTrigger::Block(block) => Ok(block.hash()),
            CosmosTrigger::Event { event_data, .. } => event_data.block().map(|b| b.hash()),
            CosmosTrigger::Transaction(transaction_data) => {
                transaction_data.block().map(|b| b.hash())
            }
            CosmosTrigger::Message(message_data) => message_data.block().map(|b| b.hash()),
        }
    }

    fn error_context(&self) -> std::string::String {
        match self {
            CosmosTrigger::Block(..) => {
                if let (Ok(block_number), Ok(block_hash)) = (self.block_number(), self.block_hash())
                {
                    format!("block #{block_number}, hash {block_hash}")
                } else {
                    "block".to_string()
                }
            }
            CosmosTrigger::Event { event_data, origin } => {
                if let (Ok(event), Ok(block_number), Ok(block_hash)) =
                    (event_data.event(), self.block_number(), self.block_hash())
                {
                    format!(
                        "event type {}, origin: {:?}, block #{block_number}, hash {block_hash}",
                        event.event_type, origin,
                    )
                } else {
                    "event".to_string()
                }
            }
            CosmosTrigger::Transaction(transaction_data) => {
                if let (Ok(block_number), Ok(block_hash), Ok(response_deliver_tx)) = (
                    self.block_number(),
                    self.block_hash(),
                    transaction_data.response_deliver_tx(),
                ) {
                    format!(
                        "block #{block_number}, hash {block_hash}, transaction log: {}",
                        response_deliver_tx.log
                    )
                } else {
                    "transaction".to_string()
                }
            }
            CosmosTrigger::Message(message_data) => {
                if let (Ok(message), Ok(block_number), Ok(block_hash)) = (
                    message_data.message(),
                    self.block_number(),
                    self.block_hash(),
                ) {
                    format!(
                        "message type {}, block #{block_number}, hash {block_hash}",
                        message.type_url,
                    )
                } else {
                    "message".to_string()
                }
            }
        }
    }
}

impl Ord for CosmosTrigger {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Events have no intrinsic ordering information, so we keep the order in
            // which they are included in the `events` field
            (Self::Event { .. }, Self::Event { .. }) => Ordering::Equal,

            // Keep the order when comparing two message triggers
            (Self::Message(..), Self::Message(..)) => Ordering::Equal,

            // Transactions are ordered by their index inside the block
            (Self::Transaction(a), Self::Transaction(b)) => {
                if let (Ok(a_tx_result), Ok(b_tx_result)) = (a.tx_result(), b.tx_result()) {
                    a_tx_result.index.cmp(&b_tx_result.index)
                } else {
                    Ordering::Equal
                }
            }

            // Keep the order when comparing two block triggers
            (Self::Block(..), Self::Block(..)) => Ordering::Equal,

            // Event triggers always come first
            (Self::Event { .. }, _) => Ordering::Greater,
            (_, Self::Event { .. }) => Ordering::Less,

            // Block triggers always come last
            (Self::Block(..), _) => Ordering::Less,
            (_, Self::Block(..)) => Ordering::Greater,

            // Message triggers before Transaction triggers
            (Self::Message(..), Self::Transaction(..)) => Ordering::Greater,
            (Self::Transaction(..), Self::Message(..)) => Ordering::Less,
        }
    }
}

impl PartialOrd for CosmosTrigger {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TriggerData for CosmosTrigger {
    fn error_context(&self) -> String {
        self.error_context()
    }

    fn address_match(&self) -> Option<&[u8]> {
        None
    }
}

impl MappingTriggerTrait for CosmosTrigger {
    fn error_context(&self) -> String {
        self.error_context()
    }
}
#[cfg(test)]
mod tests {
    use crate::codec::TxResult;

    use super::*;

    #[test]
    fn test_cosmos_trigger_ordering() {
        let event_trigger = CosmosTrigger::Event {
            event_data: Arc::<codec::EventData>::new(codec::EventData {
                ..Default::default()
            }),
            origin: EventOrigin::BeginBlock,
        };
        let other_event_trigger = CosmosTrigger::Event {
            event_data: Arc::<codec::EventData>::new(codec::EventData {
                ..Default::default()
            }),
            origin: EventOrigin::BeginBlock,
        };
        let message_trigger =
            CosmosTrigger::Message(Arc::<codec::MessageData>::new(codec::MessageData {
                ..Default::default()
            }));
        let other_message_trigger =
            CosmosTrigger::Message(Arc::<codec::MessageData>::new(codec::MessageData {
                ..Default::default()
            }));
        let transaction_trigger = CosmosTrigger::Transaction(Arc::<codec::TransactionData>::new(
            codec::TransactionData {
                block: None,
                tx: Some(TxResult {
                    index: 1,
                    ..Default::default()
                }),
            },
        ));
        let other_transaction_trigger = CosmosTrigger::Transaction(
            Arc::<codec::TransactionData>::new(codec::TransactionData {
                block: None,
                tx: Some(TxResult {
                    index: 2,
                    ..Default::default()
                }),
            }),
        );
        let block_trigger = CosmosTrigger::Block(Arc::<codec::Block>::new(codec::Block {
            ..Default::default()
        }));
        let other_block_trigger = CosmosTrigger::Block(Arc::<codec::Block>::new(codec::Block {
            ..Default::default()
        }));

        assert_eq!(event_trigger.cmp(&block_trigger), Ordering::Greater);
        assert_eq!(event_trigger.cmp(&transaction_trigger), Ordering::Greater);
        assert_eq!(event_trigger.cmp(&message_trigger), Ordering::Greater);
        assert_eq!(event_trigger.cmp(&other_event_trigger), Ordering::Equal);

        assert_eq!(message_trigger.cmp(&block_trigger), Ordering::Greater);
        assert_eq!(message_trigger.cmp(&transaction_trigger), Ordering::Greater);
        assert_eq!(message_trigger.cmp(&other_message_trigger), Ordering::Equal);
        assert_eq!(message_trigger.cmp(&event_trigger), Ordering::Less);

        assert_eq!(transaction_trigger.cmp(&block_trigger), Ordering::Greater);
        assert_eq!(
            transaction_trigger.cmp(&other_transaction_trigger),
            Ordering::Less
        );
        assert_eq!(
            other_transaction_trigger.cmp(&transaction_trigger),
            Ordering::Greater
        );
        assert_eq!(transaction_trigger.cmp(&message_trigger), Ordering::Less);
        assert_eq!(transaction_trigger.cmp(&event_trigger), Ordering::Less);

        assert_eq!(block_trigger.cmp(&other_block_trigger), Ordering::Equal);
        assert_eq!(block_trigger.cmp(&transaction_trigger), Ordering::Less);
        assert_eq!(block_trigger.cmp(&message_trigger), Ordering::Less);
        assert_eq!(block_trigger.cmp(&event_trigger), Ordering::Less);
    }
}
