use crate::{
    blocks::importer_and_db_source::BlockSerializer,
    protobuf_types::{
        Block as ProtoBlock, V1Block as ProtoV1Block,
        block::VersionedBlock as ProtoVersionedBlock,
    },
};
#[cfg(feature = "fault-proving")]
use fuel_core_types::fuel_types::ChainId;

use fuel_core_types::{
    blockchain::{
        block::Block as FuelBlock,
    },
};
use fuel_core_types::fuel_tx::Receipt as FuelReceipt;
use crate::blocks::importer_and_db_source::serializer_adapter::fuel_to_proto_conversions::{proto_header_from_header, proto_receipt_from_receipt, proto_tx_from_tx};

#[derive(Clone)]
pub struct SerializerAdapter;

impl BlockSerializer for SerializerAdapter {
    type Block = ProtoBlock;

    fn serialize_block(
        &self,
        block: &FuelBlock,
        receipts: &[FuelReceipt],
    ) -> crate::result::Result<Self::Block> {
        let proto_header = proto_header_from_header(block.header());
        match &block {
            FuelBlock::V1(_) => {
                let proto_v1_block = ProtoV1Block {
                    header: Some(proto_header),
                    transactions: block
                        .transactions()
                        .iter()
                        .map(proto_tx_from_tx)
                        .collect(),
                    receipts: receipts.iter().map(proto_receipt_from_receipt).collect(),
                };
                Ok(ProtoBlock {
                    versioned_block: Some(ProtoVersionedBlock::V1(proto_v1_block)),
                })
            }
        }
    }
}

pub mod fuel_to_proto_conversions;
pub mod proto_to_fuel_conversions;

// TODO: Add coverage for V2 Block stuff
//   https://github.com/FuelLabs/fuel-core/issues/3139
#[cfg(not(feature = "fault-proving"))]
#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::test_helpers::{arb_block, arb_receipts};
    use proptest::prelude::*;
    use crate::blocks::importer_and_db_source::serializer_adapter::proto_to_fuel_conversions::fuel_block_from_protobuf;

    proptest! {
            #![proptest_config(ProptestConfig {
      cases: 1, .. ProptestConfig::default()
    })]
          #[test]
          fn serialize_block__roundtrip(
            (block, msg_ids, event_inbox_root) in arb_block(),
            receipts in arb_receipts())
          {
              // given
              let serializer = SerializerAdapter;

              // when
              let proto_block = serializer.serialize_block(&block, &receipts).unwrap();

              // then
              let (deserialized_block, deserialized_receipts) = fuel_block_from_protobuf(proto_block, &msg_ids, event_inbox_root).unwrap();
              assert_eq!(block, deserialized_block);
              assert_eq!(receipts, deserialized_receipts);
          }
      }

    #[test]
    #[ignore]
    fn _dummy() {}
}
