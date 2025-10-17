use crate::{
    blocks::importer_and_db_source::BlockSerializer,
    protobuf_types::{
        Block as ProtoBlock,
        Header as ProtoHeader,
        Policies as ProtoPolicies,
        ScriptTx as ProtoScriptTx,
        Transaction as ProtoTransaction,
        V1Block as ProtoV1Block,
        V1Header as ProtoV1Header,
        V2Header as ProtoV2Header,
        block::{
            VersionedBlock as ProtoVersionedBlock,
            VersionedBlock,
        },
        header::VersionedHeader as ProtoVersionedHeader,
        transaction::Variant as ProtoTransactionVariant,
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
#[cfg(feature = "fault-proving")]
use fuel_core_types::blockchain::header::BlockHeaderV2;
use fuel_core_types::{
    blockchain::{
        block::Block as FuelBlock,
        consensus,
        header::{
            ApplicationHeader,
            BlockHeader,
            BlockHeaderV1,
            ConsensusHeader,
            GeneratedConsensusFields,
            PartialBlockHeader,
        },
        primitives::{
            BlockId,
            DaBlockHeight,
            Empty,
        },
    },
    fuel_tx::{
        Bytes32,
        Transaction as FuelTransaction,
        field::{
            Policies as _,
            ReceiptsRoot as _,
            Script as _,
            ScriptData as _,
            ScriptGasLimit as _,
            Witnesses as _,
        },
        policies::PolicyType,
    },
    fuel_types::ChainId,
    tai64,
};

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
    let block_id = header.id();
    let consensus = *header.consensus();
    let versioned_header = match header {
        BlockHeader::V1(header) => {
            let proto_v1_header =
                proto_v1_header_from_v1_header(consensus, block_id, header);
            ProtoVersionedHeader::V1(proto_v1_header)
        }
        #[cfg(feature = "fault-proving")]
        BlockHeader::V2(header) => {
            let proto_v2_header =
                proto_v2_header_from_v2_header(consensus, block_id, header);
            ProtoVersionedHeader::V2(proto_v2_header)
        }
    };

    ProtoHeader {
        versioned_header: Some(versioned_header),
    }
}

fn proto_v1_header_from_v1_header(
    consensus: ConsensusHeader<GeneratedConsensusFields>,
    block_id: BlockId,
    header: BlockHeaderV1,
) -> ProtoV1Header {
    let application = header.application();
    let generated = application.generated;

    ProtoV1Header {
        da_height: application.da_height.0,
        consensus_parameters_version: application.consensus_parameters_version,
        state_transition_bytecode_version: application.state_transition_bytecode_version,
        transactions_count: u32::from(generated.transactions_count),
        message_receipt_count: generated.message_receipt_count,
        transactions_root: bytes32_to_vec(&generated.transactions_root),
        message_outbox_root: bytes32_to_vec(&generated.message_outbox_root),
        event_inbox_root: bytes32_to_vec(&generated.event_inbox_root),
        prev_root: bytes32_to_vec(&consensus.prev_root),
        height: u32::from(consensus.height),
        time: consensus.time.0,
        application_hash: bytes32_to_vec(&consensus.generated.application_hash),
        block_id: Some(block_id.as_slice().to_vec()),
    }
}

#[cfg(feature = "fault-proving")]
fn proto_v2_header_from_v2_header(
    consensus: ConsensusHeader<GeneratedConsensusFields>,
    block_id: BlockId,
    header: BlockHeaderV2,
) -> ProtoV2Header {
    let application = *header.application();
    let generated = application.generated;

    ProtoV2Header {
        da_height: saturating_u64_to_u32(application.da_height.0),
        consensus_parameters_version: application.consensus_parameters_version,
        state_transition_bytecode_version: application.state_transition_bytecode_version,
        transactions_count: u32::from(generated.transactions_count),
        message_receipt_count: generated.message_receipt_count,
        transactions_root: bytes32_to_vec(&generated.transactions_root),
        message_outbox_root: bytes32_to_vec(&generated.message_outbox_root),
        event_inbox_root: bytes32_to_vec(&generated.event_inbox_root),
        tx_id_commitment: bytes32_to_vec(&generated.tx_id_commitment),
        prev_root: bytes32_to_vec(&consensus.prev_root),
        height: u32::from(consensus.height),
        time: consensus.time.0.to_be_bytes().to_vec(),
        application_hash: bytes32_to_vec(&consensus.generated.application_hash),
        block_id: Some(block_id.as_slice().to_vec()),
    }
}

fn proto_tx_from_tx(tx: FuelTransaction) -> ProtoTransaction {
    match tx {
        FuelTransaction::Script(script) => {
            let proto_script = ProtoScriptTx {
                script_gas_limit: *script.script_gas_limit(),
                receipts_root: bytes32_to_vec(script.receipts_root()),
                script: script.script().clone(),
                script_data: script.script_data().clone(),
                policies: Some(proto_policies_from_policies(script.policies())),
                inputs: Vec::new(),
                outputs: Vec::new(),
                witnesses: script
                    .witnesses()
                    .iter()
                    .map(|witness| witness.as_ref().to_vec())
                    .collect(),
                metadata: None,
            };

            ProtoTransaction {
                variant: Some(ProtoTransactionVariant::Script(proto_script)),
            }
        }
        _ => ProtoTransaction { variant: None },
    }
}

fn proto_policies_from_policies(
    policies: &fuel_core_types::fuel_tx::policies::Policies,
) -> ProtoPolicies {
    const POLICY_ORDER: [PolicyType; 5] = [
        PolicyType::Tip,
        PolicyType::WitnessLimit,
        PolicyType::Maturity,
        PolicyType::MaxFee,
        PolicyType::Expiration,
    ];

    let values = POLICY_ORDER
        .iter()
        .map(|policy_type| policies.get(*policy_type).unwrap_or_default())
        .collect();

    ProtoPolicies {
        bits: policies.bits(),
        values,
    }
}

fn bytes32_to_vec(bytes: &fuel_core_types::fuel_types::Bytes32) -> Vec<u8> {
    bytes.as_ref().to_vec()
}

fn saturating_u64_to_u32(value: u64) -> u32 {
    value.min(u32::MAX as u64) as u32
}

#[cfg(test)]
pub fn fuel_block_from_protobuf(proto_block: ProtoBlock) -> Result<FuelBlock> {
    let versioned_block = proto_block
        .versioned_block
        .ok_or_else(|| anyhow::anyhow!("Missing protobuf versioned_block"))
        .map_err(Error::Serialization)?;
    let partial_header = match &versioned_block {
        ProtoVersionedBlock::V1(v1_block) => {
            let proto_header = v1_block
                .header
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Missing protobuf header"))
                .map_err(Error::Serialization)?;
            partial_header_from_proto_header(proto_header)?
        }
    };
    let txs = match versioned_block {
        VersionedBlock::V1(v1_inner) => v1_inner
            .transactions
            .iter()
            .map(tx_from_proto_tx)
            .collect::<Result<_>>()?,
    };
    FuelBlock::new(
        partial_header,
        txs,
        &[],
        Bytes32::default(),
        #[cfg(feature = "fault-proving")]
        &ChainId::default(),
    )
    .map_err(|e| anyhow!(e))
    .map_err(Error::Serialization)
}

pub fn partial_header_from_proto_header(
    proto_header: ProtoHeader,
) -> Result<PartialBlockHeader> {
    let partial_header = PartialBlockHeader {
        consensus: proto_header_to_empty_consensus_header(&proto_header)?,
        application: proto_header_to_empty_application_header(&proto_header)?,
    };
    Ok(partial_header)
}

pub fn tx_from_proto_tx(proto_tx: &ProtoTransaction) -> Result<FuelTransaction> {
    todo!()
}

pub fn proto_header_to_empty_application_header(
    proto_header: &ProtoHeader,
) -> Result<ApplicationHeader<Empty>> {
    match proto_header.versioned_header.clone() {
        Some(ProtoVersionedHeader::V1(header)) => {
            let app_header = ApplicationHeader {
                da_height: DaBlockHeight::from(header.da_height),
                consensus_parameters_version: header.consensus_parameters_version,
                state_transition_bytecode_version: header
                    .state_transition_bytecode_version,
                generated: Empty {},
            };
            Ok(app_header)
        }
        Some(ProtoVersionedHeader::V2(header)) => {
            cfg!(feature = "fault-proving");
            {
                let app_header = ApplicationHeader {
                    da_height: DaBlockHeight::from(header.da_height),
                    consensus_parameters_version: header.consensus_parameters_version,
                    state_transition_bytecode_version: header
                        .state_transition_bytecode_version,
                    generated: Empty {},
                };
                return Ok(app_header);
            }
            cfg!(not(feature = "fault-proving"));
            {
                Err(anyhow!("V2 headers require the 'fault-proving' feature"))
                    .map_err(Error::Serialization)
            }
        }
        None => Err(anyhow!("Missing protobuf versioned_header"))
            .map_err(Error::Serialization),
    }
}

/// Alias the consensus header into an empty one.
pub fn proto_header_to_empty_consensus_header(
    proto_header: &ProtoHeader,
) -> Result<ConsensusHeader<Empty>> {
    match proto_header.versioned_header.clone() {
        Some(ProtoVersionedHeader::V1(header)) => {
            let consensus_header = ConsensusHeader {
                prev_root: *Bytes32::from_bytes_ref_checked(&header.prev_root).ok_or(
                    Error::Serialization(anyhow!("Could create `Bytes32` from bytes")),
                )?,
                height: header.height.into(),
                time: tai64::Tai64(header.time),
                generated: Empty {},
            };
            Ok(consensus_header)
        }
        Some(ProtoVersionedHeader::V2(header)) => {
            cfg!(feature = "fault-proving");
            {
                let consensus_header = ConsensusHeader {
                    prev_root: *Bytes32::from_bytes_ref_checked(&header.prev_root)
                        .ok_or(Error::Serialization(anyhow!(
                            "Could create `Bytes32` from bytes"
                        )))?,
                    height: header.height.into(),
                    time: tai64::Tai64(header.time),
                    generated: Empty {},
                };
                return Ok(consensus_header);
            }
            cfg!(not(feature = "fault-proving"));
            {
                Err(anyhow!("V2 headers require the 'fault-proving' feature"))
                    .map_err(Error::Serialization)
            }
        }
        None => Err(anyhow!("Missing protobuf versioned_header"))
            .map_err(Error::Serialization),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_block__roundtrip() {
        // given
        let serializer = SerializerAdapter;
        let mut fuel_block = FuelBlock::default();
        let transaction_tree =
            fuel_core_types::fuel_merkle::binary::root_calculator::MerkleRootCalculator::new(
            );
        let root = transaction_tree.root().into();
        fuel_block.header_mut().set_transaction_root(root);
        fuel_block.header_mut().set_message_outbox_root(root);

        // when
        let proto_block = serializer.serialize_block(&fuel_block).unwrap();

        // then
        let deserialized_block = fuel_block_from_protobuf(proto_block).unwrap();
        assert_eq!(fuel_block, deserialized_block);
    }
}
