use crate::{
    blocks::old_block_source::{
        BlockConverter,
        convertor_adapter::fuel_to_proto_conversions::{
            proto_header_from_header,
            proto_receipts_from_receipts,
            proto_tx_from_tx,
        },
    },
    protobuf_types::{
        Block as ProtoBlock,
        V1Block as ProtoV1Block,
        block::VersionedBlock as ProtoVersionedBlock,
    },
};
#[cfg(feature = "fault-proving")]
use fuel_core_types::fuel_types::ChainId;
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_tx::Receipt as FuelReceipt,
};
use prost::Message;
use std::sync::Arc;

#[derive(Clone)]
pub struct ProtobufBlockConverter;

impl BlockConverter for ProtobufBlockConverter {
    type Block = Arc<[u8]>;

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
                let proto_block = ProtoBlock {
                    versioned_block: Some(ProtoVersionedBlock::V1(proto_v1_block)),
                };
                let mut bytes = Vec::new();
                proto_block
                    .encode(&mut bytes)
                    .map_err(crate::result::Error::serialization_error)?;
                Ok(bytes.into())
            }
        }
    }
}

pub mod fuel_to_proto_conversions;
pub mod proto_to_fuel_conversions;

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::old_block_source::convertor_adapter::proto_to_fuel_conversions::fuel_block_from_protobuf;
    #[cfg(feature = "fault-proving")]
    use fuel_core_types::blockchain::header::BlockHeaderV1;
    use fuel_core_types::{
        blockchain::header::{
            BlockHeader,
            PartialBlockHeader,
        },
        fuel_tx::Transaction,
        test_helpers::arb_block,
    };
    use proptest::prelude::*;

    proptest! {
            #![proptest_config(ProptestConfig {
      cases: 1, .. ProptestConfig::default()
    })]
          #[test]
          fn serialize_block__roundtrip(
            (block, receipts) in arb_block())
          {
              // given
              let convertor = ProtobufBlockConverter;

              // when
              let receipts = vec![receipts];
              let bytes = convertor.convert_block(&block, &receipts).unwrap();
              let proto_block = ProtoBlock::decode(&*bytes).unwrap();

              // then
              let (deserialized_block, deserialized_receipts) = fuel_block_from_protobuf(proto_block).unwrap();
              assert_eq!(block, deserialized_block);
              assert_eq!(receipts, deserialized_receipts);
          }
      }

    fn block_with_receipts() -> (FuelBlock, Vec<Vec<FuelReceipt>>) {
        let message = FuelReceipt::message_out(
            &[1; 32].into(),
            0,
            [2; 32].into(),
            [3; 32].into(),
            4,
            vec![5, 6],
        );
        let reverted_message = FuelReceipt::message_out(
            &[7; 32].into(),
            1,
            [8; 32].into(),
            [9; 32].into(),
            10,
            vec![11],
        );
        let block = FuelBlock::new(
            PartialBlockHeader::default(),
            vec![Transaction::default(); 3],
            &[message.message_id().unwrap()],
            [12; 32].into(),
            #[cfg(feature = "fault-proving")]
            &ChainId::new(42),
        )
        .unwrap();
        let receipts = vec![
            vec![message],
            vec![],
            vec![
                reverted_message,
                FuelReceipt::revert(Default::default(), 1, 2, 3),
            ],
        ];
        (block, receipts)
    }

    fn v1_block() -> (FuelBlock, Vec<Vec<FuelReceipt>>) {
        let (block, receipts) = block_with_receipts();
        #[cfg(feature = "fault-proving")]
        let block = {
            let (header, transactions) = block.into_inner();
            let BlockHeader::V2(v2) = &header else {
                unreachable!()
            };
            let mut v1 = BlockHeaderV1::default();
            v1.set_application_header(v2.application().into());
            let mut v1 = BlockHeader::V1(v1);
            v1.set_consensus_header(*header.consensus());
            v1.recalculate_metadata();
            FuelBlock::try_from_executed(v1, transactions).unwrap()
        };
        (block, receipts)
    }

    fn assert_roundtrip(block: &FuelBlock, receipts: &[Vec<FuelReceipt>]) {
        let bytes = ProtobufBlockConverter
            .convert_block(block, receipts)
            .unwrap();
        let proto = ProtoBlock::decode(&*bytes).unwrap();
        let (decoded, decoded_receipts) = fuel_block_from_protobuf(proto).unwrap();
        assert_eq!(block, &decoded);
        assert_eq!(block.id(), decoded.id());
        assert_eq!(receipts, decoded_receipts);
    }

    #[test]
    fn serialize_v1_block__roundtrip() {
        let (block, receipts) = v1_block();
        assert!(matches!(block.header(), BlockHeader::V1(_)));
        assert_roundtrip(&block, &receipts);
    }

    #[test]
    fn deserialize_v1_block__normalizes_generated_fields() {
        let (block, receipts) = v1_block();
        let bytes = ProtobufBlockConverter
            .convert_block(&block, &receipts)
            .unwrap();
        let mut proto = ProtoBlock::decode(&*bytes).unwrap();
        let Some(ProtoVersionedBlock::V1(inner)) = &mut proto.versioned_block else {
            unreachable!()
        };
        let Some(crate::protobuf_types::header::VersionedHeader::V1(header)) =
            &mut inner.header.as_mut().unwrap().versioned_header
        else {
            unreachable!()
        };
        header.transactions_count = u32::MAX;
        header.transactions_root.clear();
        header.message_receipt_count = u32::MAX;
        header.message_outbox_root.clear();
        header.application_hash.clear();
        header.block_id = None;
        let (decoded, decoded_receipts) = fuel_block_from_protobuf(proto).unwrap();
        assert_eq!(block, decoded);
        assert_eq!(receipts, decoded_receipts);
    }

    #[cfg(feature = "fault-proving")]
    #[test]
    fn serialize_v2_block__roundtrip_nonzero_chain_id() {
        let (block, receipts) = block_with_receipts();
        assert!(matches!(block.header(), BlockHeader::V2(_)));
        assert!(!block.transactions().is_empty());
        assert_roundtrip(&block, &receipts);
    }

    #[cfg(feature = "fault-proving")]
    #[test]
    fn deserialize_v2_block__rejects_malformed_commitment() {
        let (block, receipts) = block_with_receipts();
        let bytes = ProtobufBlockConverter
            .convert_block(&block, &receipts)
            .unwrap();
        let mut proto = ProtoBlock::decode(&*bytes).unwrap();
        let Some(ProtoVersionedBlock::V1(inner)) = &mut proto.versioned_block else {
            unreachable!()
        };
        let Some(crate::protobuf_types::header::VersionedHeader::V2(header)) =
            &mut inner.header.as_mut().unwrap().versioned_header
        else {
            unreachable!()
        };
        header.tx_id_commitment.pop();
        assert!(fuel_block_from_protobuf(proto).is_err());
    }
}
