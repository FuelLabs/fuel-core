pub(crate) use crate::protobuf::pbcodec::*;

use graph::{
    blockchain::{
        Block as BlockchainBlock,
        BlockPtr,
    },
    prelude::{
        anyhow::anyhow,
        BlockNumber,
        Error,
    },
};

use std::convert::TryFrom;

impl Block {
    pub fn parent_ptr(&self) -> Option<BlockPtr> {
        match self.height {
            0 => None,
            _ => Some(BlockPtr {
                hash: self.prev_id.clone().into(),
                number: self.number().saturating_sub(1),
            }),
        }
    }
}

impl<'a> From<&'a Block> for BlockPtr {
    fn from(b: &'a Block) -> BlockPtr {
        BlockPtr::new(b.id.clone().into(), b.number())
    }
}

impl BlockchainBlock for Block {
    fn ptr(&self) -> BlockPtr {
        BlockPtr::try_from(self).unwrap()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        self.parent_ptr()
    }

    fn number(&self) -> i32 {
        BlockNumber::try_from(self.height).unwrap()
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::codec::TxResult;
//
//     use super::*;
//
//     #[test]
//     fn test_cosmos_trigger_ordering() {
//         let event_trigger = CosmosTrigger::Event {
//             event_data: Arc::<codec::EventData>::new(codec::EventData {
//                 ..Default::default()
//             }),
//             origin: EventOrigin::BeginBlock,
//         };
//         let other_event_trigger = CosmosTrigger::Event {
//             event_data: Arc::<codec::EventData>::new(codec::EventData {
//                 ..Default::default()
//             }),
//             origin: EventOrigin::BeginBlock,
//         };
//         let message_trigger =
//             CosmosTrigger::Message(Arc::<codec::MessageData>::new(codec::MessageData {
//                 ..Default::default()
//             }));
//         let other_message_trigger =
//             CosmosTrigger::Message(Arc::<codec::MessageData>::new(codec::MessageData {
//                 ..Default::default()
//             }));
//         let transaction_trigger = CosmosTrigger::Transaction(Arc::<codec::TransactionData>::new(
//             codec::TransactionData {
//                 block: None,
//                 tx: Some(TxResult {
//                     index: 1,
//                     ..Default::default()
//                 }),
//             },
//         ));
//         let other_transaction_trigger = CosmosTrigger::Transaction(
//             Arc::<codec::TransactionData>::new(codec::TransactionData {
//                 block: None,
//                 tx: Some(TxResult {
//                     index: 2,
//                     ..Default::default()
//                 }),
//             }),
//         );
//         let block_trigger = CosmosTrigger::Block(Arc::<codec::Block>::new(codec::Block {
//             ..Default::default()
//         }));
//         let other_block_trigger = CosmosTrigger::Block(Arc::<codec::Block>::new(codec::Block {
//             ..Default::default()
//         }));
//
//         assert_eq!(event_trigger.cmp(&block_trigger), Ordering::Greater);
//         assert_eq!(event_trigger.cmp(&transaction_trigger), Ordering::Greater);
//         assert_eq!(event_trigger.cmp(&message_trigger), Ordering::Greater);
//         assert_eq!(event_trigger.cmp(&other_event_trigger), Ordering::Equal);
//
//         assert_eq!(message_trigger.cmp(&block_trigger), Ordering::Greater);
//         assert_eq!(message_trigger.cmp(&transaction_trigger), Ordering::Greater);
//         assert_eq!(message_trigger.cmp(&other_message_trigger), Ordering::Equal);
//         assert_eq!(message_trigger.cmp(&event_trigger), Ordering::Less);
//
//         assert_eq!(transaction_trigger.cmp(&block_trigger), Ordering::Greater);
//         assert_eq!(
//             transaction_trigger.cmp(&other_transaction_trigger),
//             Ordering::Less
//         );
//         assert_eq!(
//             other_transaction_trigger.cmp(&transaction_trigger),
//             Ordering::Greater
//         );
//         assert_eq!(transaction_trigger.cmp(&message_trigger), Ordering::Less);
//         assert_eq!(transaction_trigger.cmp(&event_trigger), Ordering::Less);
//
//         assert_eq!(block_trigger.cmp(&other_block_trigger), Ordering::Equal);
//         assert_eq!(block_trigger.cmp(&transaction_trigger), Ordering::Less);
//         assert_eq!(block_trigger.cmp(&message_trigger), Ordering::Less);
//         assert_eq!(block_trigger.cmp(&event_trigger), Ordering::Less);
//     }
// }
