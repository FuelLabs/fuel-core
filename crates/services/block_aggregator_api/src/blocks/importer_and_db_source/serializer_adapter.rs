use crate::{
    blocks::importer_and_db_source::BlockSerializer,
    result::Error,
};

use crate::protobuf_types::{
    Block as ProtoBlock,
    Header as ProtoHeader,
    Transaction as ProtoTransaction,
    V1Block as ProtoV1Block,
    block::VersionedBlock as ProtoVersionedBlock,
};
use anyhow::anyhow;
use fuel_core_types::{
    blockchain::{
        block::{
            Block as FuelBlock,
            BlockV1,
        },
        header::BlockHeader,
    },
    fuel_tx::Transaction as FuelTransaction,
};
use postcard::to_allocvec;

#[derive(Clone)]
pub struct SerializerAdapter;

impl BlockSerializer for SerializerAdapter {
    type Block = ProtoBlock;

    fn serialize_block(&self, block: &FuelBlock) -> crate::result::Result<Self::Block> {
        // TODO: Should this be owned to begin with?
        let (header, txs) = block.clone().into_inner();
        let proto_header = proto_header_from_header(header);
        match &block {
            FuelBlock::V1(_) => {
                let proto_v1_block = ProtoV1Block {
                    header: Some(proto_header),
                    transactions: txs.into_iter().map(proto_tx_from_tx).collect(),
                };
                Ok(ProtoBlock {
                    versioned_block: Some(ProtoVersionedBlock::V1(proto_v1_block)),
                })
            }
        }
    }
}

fn proto_header_from_header(header: BlockHeader) -> ProtoHeader {
    todo!()
}

fn proto_tx_from_tx(tx: FuelTransaction) -> ProtoTransaction {
    todo!()
}
