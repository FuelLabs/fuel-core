use crate::{
    blocks::old_block_source::BlockConvector,
    protobuf_types::{
        Block as ProtoBlock,
        V1Block as ProtoV1Block,
        block::VersionedBlock as ProtoVersionedBlock,
    },
};
#[cfg(feature = "fault-proving")]
use fuel_core_types::fuel_types::ChainId;

use crate::blocks::old_block_source::convertor_adapter::fuel_to_proto_conversions::{
    proto_header_from_header,
    proto_receipts_from_receipts,
    proto_tx_from_tx,
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_tx::Receipt as FuelReceipt,
};

#[derive(Clone)]
pub struct ConvertorAdapter;

impl BlockConvector for ConvertorAdapter {
    type Block = ProtoBlock;

    fn convert_block(
        &self,
        block: &FuelBlock,
        receipts: &[Vec<FuelReceipt>],
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
                    receipts: receipts
                        .iter()
                        .map(|rs| proto_receipts_from_receipts(rs))
                        .collect(),
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
    use crate::blocks::old_block_source::convertor_adapter::proto_to_fuel_conversions::fuel_block_from_protobuf;
    use fuel_core_types::test_helpers::{
        arb_block,
        arb_receipts,
    };
    use proptest::prelude::*;

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
              let convertor = ConvertorAdapter;

              // when
              let receipts = vec![receipts];
              let proto_block = convertor.convert_block(&block, &receipts).unwrap();

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
