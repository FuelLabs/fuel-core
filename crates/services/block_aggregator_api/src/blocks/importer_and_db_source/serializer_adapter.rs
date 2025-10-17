#[cfg(feature = "fault-proving")]
use crate::protobuf_types::V2Header as ProtoV2Header;
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
        block::VersionedBlock as ProtoVersionedBlock,
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
#[cfg(all(test, feature = "fault-proving"))]
use fuel_core_types::fuel_types::ChainId;

use crate::protobuf_types::Policies;
use fuel_core_types::{
    blockchain::{
        block::Block as FuelBlock,
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
        Script,
        Transaction as FuelTransaction,
        field::{
            ChargeableBody,
            Policies as _,
            ReceiptsRoot as _,
            Script as _,
            ScriptData as _,
            ScriptGasLimit as _,
            Witnesses as _,
        },
        policies::{
            Policies as FuelPolicies,
            PoliciesBits,
            PolicyType,
        },
    },
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
        da_height: application.da_height.0,
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
        time: consensus.time.0,
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
    let mut values = [0u64; 5];
    if policies.is_set(PolicyType::Tip) {
        values[0] = policies.get(PolicyType::Tip).unwrap_or_default();
    }
    if policies.is_set(PolicyType::WitnessLimit) {
        let value = policies.get(PolicyType::WitnessLimit).unwrap_or_default();
        values[1] = value;
    }
    if policies.is_set(PolicyType::Maturity) {
        let value = policies.get(PolicyType::Maturity).unwrap_or_default();
        values[2] = value;
    }
    if policies.is_set(PolicyType::MaxFee) {
        values[3] = policies.get(PolicyType::MaxFee).unwrap_or_default();
    }
    if policies.is_set(PolicyType::Expiration) {
        values[4] = policies.get(PolicyType::Expiration).unwrap_or_default();
    }
    let bits = policies.bits();
    ProtoPolicies {
        bits,
        values: values.to_vec(),
    }
}

fn bytes32_to_vec(bytes: &fuel_core_types::fuel_types::Bytes32) -> Vec<u8> {
    bytes.as_ref().to_vec()
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
        ProtoVersionedBlock::V1(v1_inner) => v1_inner
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

pub fn tx_from_proto_tx(_proto_tx: &ProtoTransaction) -> Result<FuelTransaction> {
    match &_proto_tx.variant {
        Some(ProtoTransactionVariant::Script(_proto_script)) => {
            let ProtoScriptTx {
                script_gas_limit,
                receipts_root,
                script,
                script_data,
                policies,
                inputs,
                outputs,
                witnesses,
                metadata,
            } = _proto_script.clone();
            //         gas_limit: Word,
            //         script: Vec<u8>,
            //         script_data: Vec<u8>,
            //         policies: Policies,
            //         inputs: Vec<Input>,
            //         outputs: Vec<Output>,
            //         witnesses: Vec<Witness>,
            let fuel_policies = policies
                .map(policies_from_proto_policies)
                .unwrap_or_default();
            let script_tx = FuelTransaction::script(
                script_gas_limit,
                script,
                script_data,
                fuel_policies,
                vec![],
                vec![],
                vec![],
            );

            Ok(FuelTransaction::Script(script_tx))
        }
        _ => {
            Err(anyhow!("Unsupported transaction variant")).map_err(Error::Serialization)
        }
    }
}

//     /// Sets the `gas_price` policy.
//     pub fn with_tip(mut self, tip: Word) -> Self {
//         self.set(PolicyType::Tip, Some(tip));
//         self
//     }
//
//     /// Sets the `witness_limit` policy.
//     pub fn with_witness_limit(mut self, witness_limit: Word) -> Self {
//         self.set(PolicyType::WitnessLimit, Some(witness_limit));
//         self
//     }
//
//     /// Sets the `maturity` policy.
//     pub fn with_maturity(mut self, maturity: BlockHeight) -> Self {
//         self.set(PolicyType::Maturity, Some(*maturity.deref() as u64));
//         self
//     }
//
//     /// Sets the `expiration` policy.
//     pub fn with_expiration(mut self, expiration: BlockHeight) -> Self {
//         self.set(PolicyType::Expiration, Some(*expiration.deref() as u64));
//         self
//     }
//
//     /// Sets the `max_fee` policy.
//     pub fn with_max_fee(mut self, max_fee: Word) -> Self {
//         self.set(PolicyType::MaxFee, Some(max_fee));
//         self
//     }
//
//     /// Sets the `owner` policy.
//     pub fn with_owner(mut self, owner: Word) -> Self {
//         self.set(PolicyType::Owner, Some(owner));
//         self
//     }
//
// bitflags::bitflags! {
//     /// See https://github.com/FuelLabs/fuel-specs/blob/master/src/tx-format/policy.md#policy
//     #[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Hash)]
//     #[derive(serde::Serialize, serde::Deserialize)]
//     pub struct PoliciesBits: u32 {
//         /// If set, the gas price is present in the policies.
//         const Tip = 1 << 0;
//         /// If set, the witness limit is present in the policies.
//         const WitnessLimit = 1 << 1;
//         /// If set, the maturity is present in the policies.
//         const Maturity = 1 << 2;
//         /// If set, the max fee is present in the policies.
//         const MaxFee = 1 << 3;
//         /// If set, the expiration is present in the policies.
//         const Expiration = 1 << 4;
//         /// If set, the owner is present in the policies.
//         const Owner = 1 << 5;
//     }
// }
fn policies_from_proto_policies(proto_policies: ProtoPolicies) -> FuelPolicies {
    let ProtoPolicies { bits, values } = proto_policies;
    let mut policies = FuelPolicies::default();
    let bits =
        PoliciesBits::from_bits(bits).expect("Should be able to create from `u32`");
    if bits.contains(PoliciesBits::Tip) {
        if let Some(tip) = values.get(0) {
            policies.set(PolicyType::Tip, Some(*tip));
        }
    }
    if bits.contains(PoliciesBits::WitnessLimit) {
        if let Some(witness_limit) = values.get(1) {
            policies.set(PolicyType::WitnessLimit, Some(*witness_limit));
        }
    }
    if bits.contains(PoliciesBits::Maturity) {
        if let Some(maturity) = values.get(2) {
            policies.set(PolicyType::Maturity, Some(*maturity));
        }
    }
    if bits.contains(PoliciesBits::MaxFee) {
        if let Some(max_fee) = values.get(3) {
            policies.set(PolicyType::MaxFee, Some(*max_fee));
        }
    }
    if bits.contains(PoliciesBits::Expiration) {
        if let Some(expiration) = values.get(4) {
            policies.set(PolicyType::Expiration, Some(*expiration));
        }
    }
    policies
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
            if cfg!(feature = "fault-proving") {
                let app_header = ApplicationHeader {
                    da_height: DaBlockHeight::from(header.da_height),
                    consensus_parameters_version: header.consensus_parameters_version,
                    state_transition_bytecode_version: header
                        .state_transition_bytecode_version,
                    generated: Empty {},
                };
                Ok(app_header)
            } else {
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
            if cfg!(feature = "fault-proving") {
                let consensus_header = ConsensusHeader {
                    prev_root: *Bytes32::from_bytes_ref_checked(&header.prev_root)
                        .ok_or(Error::Serialization(anyhow!(
                            "Could create `Bytes32` from bytes"
                        )))?,
                    height: header.height.into(),
                    time: tai64::Tai64(header.time),
                    generated: Empty {},
                };
                Ok(consensus_header)
            } else {
                Err(anyhow!("V2 headers require the 'fault-proving' feature"))
                    .map_err(Error::Serialization)
            }
        }
        None => Err(anyhow!("Missing protobuf versioned_header"))
            .map_err(Error::Serialization),
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::{
        fuel_tx::{
            Blob,
            Create,
            Mint,
            Script,
            Upgrade,
            Upload,
        },
        test_helpers::arb_block,
    };
    use proptest::prelude::*;

    proptest! {
            #![proptest_config(ProptestConfig {
      cases: 1, .. ProptestConfig::default()
    })]
          #[test]
          fn serialize_block__roundtrip(block in arb_block()) {
              // given
              let serializer = SerializerAdapter;

              // when
              let proto_block = serializer.serialize_block(&block).unwrap();

              // then
              let deserialized_block = fuel_block_from_protobuf(proto_block).unwrap();
              assert_eq!(block, deserialized_block);

          }
      }

    #[test]
    #[ignore]
    fn _dummy() {}
}
