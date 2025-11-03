#[cfg(feature = "fault-proving")]
use crate::protobuf_types::V2Header as ProtoV2Header;
use crate::{
    blocks::importer_and_db_source::BlockSerializer,
    protobuf_types::{
        BlobTransaction as ProtoBlobTx,
        Block as ProtoBlock,
        ChangeOutput as ProtoChangeOutput,
        CoinOutput as ProtoCoinOutput,
        CoinPredicateInput as ProtoCoinPredicateInput,
        CoinSignedInput as ProtoCoinSignedInput,
        ContractCreatedOutput as ProtoContractCreatedOutput,
        ContractInput as ProtoContractInput,
        ContractOutput as ProtoContractOutput,
        CreateTransaction as ProtoCreateTx,
        Header as ProtoHeader,
        Input as ProtoInput,
        MessageCoinPredicateInput as ProtoMessageCoinPredicateInput,
        MessageCoinSignedInput as ProtoMessageCoinSignedInput,
        MessageDataPredicateInput as ProtoMessageDataPredicateInput,
        MessageDataSignedInput as ProtoMessageDataSignedInput,
        MintTransaction as ProtoMintTx,
        Output as ProtoOutput,
        Policies as ProtoPolicies,
        ScriptTransaction as ProtoScriptTx,
        StorageSlot as ProtoStorageSlot,
        Transaction as ProtoTransaction,
        TxPointer as ProtoTxPointer,
        UpgradeConsensusParameters as ProtoUpgradeConsensusParameters,
        UpgradePurpose as ProtoUpgradePurpose,
        UpgradeStateTransition as ProtoUpgradeStateTransition,
        UpgradeTransaction as ProtoUpgradeTx,
        UploadTransaction as ProtoUploadTx,
        UtxoId as ProtoUtxoId,
        V1Block as ProtoV1Block,
        V1Header as ProtoV1Header,
        VariableOutput as ProtoVariableOutput,
        block::VersionedBlock as ProtoVersionedBlock,
        header::VersionedHeader as ProtoVersionedHeader,
        input::Variant as ProtoInputVariant,
        output::Variant as ProtoOutputVariant,
        transaction::Variant as ProtoTransactionVariant,
        upgrade_purpose::Variant as ProtoUpgradePurposeVariant,
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
#[cfg(feature = "fault-proving")]
use fuel_core_types::{
    blockchain::header::BlockHeaderV2,
    fuel_types::ChainId,
};

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
        Address,
        BlobBody,
        Bytes32,
        Input,
        Output,
        StorageSlot,
        Transaction as FuelTransaction,
        TxPointer,
        UpgradePurpose,
        UploadBody,
        UtxoId,
        Witness,
        field::{
            BlobId as _,
            BytecodeRoot as _,
            BytecodeWitnessIndex as _,
            InputContract as _,
            Inputs,
            MintAmount as _,
            MintAssetId as _,
            MintGasPrice as _,
            OutputContract as _,
            Outputs,
            Policies as _,
            ProofSet as _,
            ReceiptsRoot as _,
            Salt as _,
            Script as _,
            ScriptData as _,
            ScriptGasLimit as _,
            StorageSlots as _,
            SubsectionIndex as _,
            SubsectionsNumber as _,
            TxPointer as TxPointerField,
            UpgradePurpose as UpgradePurposeField,
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
                inputs: script
                    .inputs()
                    .iter()
                    .cloned()
                    .map(proto_input_from_input)
                    .collect(),
                outputs: script
                    .outputs()
                    .iter()
                    .cloned()
                    .map(proto_output_from_output)
                    .collect(),
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
        FuelTransaction::Create(create) => {
            let proto_create = ProtoCreateTx {
                bytecode_witness_index: u32::from(*create.bytecode_witness_index()),
                salt: create.salt().as_ref().to_vec(),
                storage_slots: create
                    .storage_slots()
                    .iter()
                    .map(proto_storage_slot_from_storage_slot)
                    .collect(),
                policies: Some(proto_policies_from_policies(create.policies())),
                inputs: create
                    .inputs()
                    .iter()
                    .cloned()
                    .map(proto_input_from_input)
                    .collect(),
                outputs: create
                    .outputs()
                    .iter()
                    .cloned()
                    .map(proto_output_from_output)
                    .collect(),
                witnesses: create
                    .witnesses()
                    .iter()
                    .map(|witness| witness.as_ref().to_vec())
                    .collect(),
                metadata: None,
            };

            ProtoTransaction {
                variant: Some(ProtoTransactionVariant::Create(proto_create)),
            }
        }
        FuelTransaction::Mint(mint) => {
            let proto_mint = ProtoMintTx {
                tx_pointer: Some(proto_tx_pointer(mint.tx_pointer())),
                input_contract: Some(proto_contract_input_from_contract(
                    mint.input_contract(),
                )),
                output_contract: Some(proto_contract_output_from_contract(
                    mint.output_contract(),
                )),
                mint_amount: *mint.mint_amount(),
                mint_asset_id: mint.mint_asset_id().as_ref().to_vec(),
                gas_price: *mint.gas_price(),
                metadata: None,
            };

            ProtoTransaction {
                variant: Some(ProtoTransactionVariant::Mint(proto_mint)),
            }
        }
        FuelTransaction::Upgrade(upgrade) => {
            let proto_upgrade = ProtoUpgradeTx {
                purpose: Some(proto_upgrade_purpose(upgrade.upgrade_purpose())),
                policies: Some(proto_policies_from_policies(upgrade.policies())),
                inputs: upgrade
                    .inputs()
                    .iter()
                    .cloned()
                    .map(proto_input_from_input)
                    .collect(),
                outputs: upgrade
                    .outputs()
                    .iter()
                    .cloned()
                    .map(proto_output_from_output)
                    .collect(),
                witnesses: upgrade
                    .witnesses()
                    .iter()
                    .map(|witness| witness.as_ref().to_vec())
                    .collect(),
                metadata: None,
            };

            ProtoTransaction {
                variant: Some(ProtoTransactionVariant::Upgrade(proto_upgrade)),
            }
        }
        FuelTransaction::Upload(upload) => {
            let proto_upload = ProtoUploadTx {
                root: bytes32_to_vec(upload.bytecode_root()),
                witness_index: u32::from(*upload.bytecode_witness_index()),
                subsection_index: u32::from(*upload.subsection_index()),
                subsections_number: u32::from(*upload.subsections_number()),
                proof_set: upload.proof_set().iter().map(bytes32_to_vec).collect(),
                policies: Some(proto_policies_from_policies(upload.policies())),
                inputs: upload
                    .inputs()
                    .iter()
                    .cloned()
                    .map(proto_input_from_input)
                    .collect(),
                outputs: upload
                    .outputs()
                    .iter()
                    .cloned()
                    .map(proto_output_from_output)
                    .collect(),
                witnesses: upload
                    .witnesses()
                    .iter()
                    .map(|witness| witness.as_ref().to_vec())
                    .collect(),
                metadata: None,
            };

            ProtoTransaction {
                variant: Some(ProtoTransactionVariant::Upload(proto_upload)),
            }
        }
        FuelTransaction::Blob(blob) => {
            let proto_blob = ProtoBlobTx {
                blob_id: blob.blob_id().as_ref().to_vec(),
                witness_index: u32::from(*blob.bytecode_witness_index()),
                policies: Some(proto_policies_from_policies(blob.policies())),
                inputs: blob
                    .inputs()
                    .iter()
                    .cloned()
                    .map(proto_input_from_input)
                    .collect(),
                outputs: blob
                    .outputs()
                    .iter()
                    .cloned()
                    .map(proto_output_from_output)
                    .collect(),
                witnesses: blob
                    .witnesses()
                    .iter()
                    .map(|witness| witness.as_ref().to_vec())
                    .collect(),
                metadata: None,
            };

            ProtoTransaction {
                variant: Some(ProtoTransactionVariant::Blob(proto_blob)),
            }
        }
    }
}

fn proto_input_from_input(input: Input) -> ProtoInput {
    match input {
        Input::CoinSigned(coin_signed) => ProtoInput {
            variant: Some(ProtoInputVariant::CoinSigned(ProtoCoinSignedInput {
                utxo_id: Some(proto_utxo_id_from_utxo_id(&coin_signed.utxo_id)),
                owner: coin_signed.owner.as_ref().to_vec(),
                amount: coin_signed.amount,
                asset_id: coin_signed.asset_id.as_ref().to_vec(),
                tx_pointer: Some(proto_tx_pointer(&coin_signed.tx_pointer)),
                witness_index: coin_signed.witness_index.into(),
                predicate_gas_used: 0,
                predicate: vec![],
                predicate_data: vec![],
            })),
        },
        Input::CoinPredicate(coin_predicate) => ProtoInput {
            variant: Some(ProtoInputVariant::CoinPredicate(ProtoCoinPredicateInput {
                utxo_id: Some(proto_utxo_id_from_utxo_id(&coin_predicate.utxo_id)),
                owner: coin_predicate.owner.as_ref().to_vec(),
                amount: coin_predicate.amount,
                asset_id: coin_predicate.asset_id.as_ref().to_vec(),
                tx_pointer: Some(proto_tx_pointer(&coin_predicate.tx_pointer)),
                witness_index: 0,
                predicate_gas_used: coin_predicate.predicate_gas_used,
                predicate: coin_predicate.predicate.as_ref().to_vec(),
                predicate_data: coin_predicate.predicate_data.as_ref().to_vec(),
            })),
        },
        Input::Contract(contract) => ProtoInput {
            variant: Some(ProtoInputVariant::Contract(ProtoContractInput {
                utxo_id: Some(proto_utxo_id_from_utxo_id(&contract.utxo_id)),
                balance_root: bytes32_to_vec(&contract.balance_root),
                state_root: bytes32_to_vec(&contract.state_root),
                tx_pointer: Some(proto_tx_pointer(&contract.tx_pointer)),
                contract_id: contract.contract_id.as_ref().to_vec(),
            })),
        },
        Input::MessageCoinSigned(message) => ProtoInput {
            variant: Some(ProtoInputVariant::MessageCoinSigned(
                ProtoMessageCoinSignedInput {
                    sender: message.sender.as_ref().to_vec(),
                    recipient: message.recipient.as_ref().to_vec(),
                    amount: message.amount,
                    nonce: message.nonce.as_ref().to_vec(),
                    witness_index: message.witness_index.into(),
                    predicate_gas_used: 0,
                    data: Vec::new(),
                    predicate: Vec::new(),
                    predicate_data: Vec::new(),
                },
            )),
        },
        Input::MessageCoinPredicate(message) => ProtoInput {
            variant: Some(ProtoInputVariant::MessageCoinPredicate(
                ProtoMessageCoinPredicateInput {
                    sender: message.sender.as_ref().to_vec(),
                    recipient: message.recipient.as_ref().to_vec(),
                    amount: message.amount,
                    nonce: message.nonce.as_ref().to_vec(),
                    witness_index: 0,
                    predicate_gas_used: message.predicate_gas_used,
                    data: Vec::new(),
                    predicate: message.predicate.as_ref().to_vec(),
                    predicate_data: message.predicate_data.as_ref().to_vec(),
                },
            )),
        },
        Input::MessageDataSigned(message) => ProtoInput {
            variant: Some(ProtoInputVariant::MessageDataSigned(
                ProtoMessageDataSignedInput {
                    sender: message.sender.as_ref().to_vec(),
                    recipient: message.recipient.as_ref().to_vec(),
                    amount: message.amount,
                    nonce: message.nonce.as_ref().to_vec(),
                    witness_index: message.witness_index.into(),
                    predicate_gas_used: 0,
                    data: message.data.as_ref().to_vec(),
                    predicate: Vec::new(),
                    predicate_data: Vec::new(),
                },
            )),
        },
        Input::MessageDataPredicate(message) => ProtoInput {
            variant: Some(ProtoInputVariant::MessageDataPredicate(
                ProtoMessageDataPredicateInput {
                    sender: message.sender.as_ref().to_vec(),
                    recipient: message.recipient.as_ref().to_vec(),
                    amount: message.amount,
                    nonce: message.nonce.as_ref().to_vec(),
                    witness_index: 0,
                    predicate_gas_used: message.predicate_gas_used,
                    data: message.data.as_ref().to_vec(),
                    predicate: message.predicate.as_ref().to_vec(),
                    predicate_data: message.predicate_data.as_ref().to_vec(),
                },
            )),
        },
    }
}

fn proto_utxo_id_from_utxo_id(utxo_id: &UtxoId) -> ProtoUtxoId {
    ProtoUtxoId {
        tx_id: utxo_id.tx_id().as_ref().to_vec(),
        output_index: utxo_id.output_index().into(),
    }
}

fn proto_tx_pointer(tx_pointer: &TxPointer) -> ProtoTxPointer {
    ProtoTxPointer {
        block_height: tx_pointer.block_height().into(),
        tx_index: tx_pointer.tx_index().into(),
    }
}

fn proto_storage_slot_from_storage_slot(slot: &StorageSlot) -> ProtoStorageSlot {
    ProtoStorageSlot {
        key: slot.key().as_ref().to_vec(),
        value: slot.value().as_ref().to_vec(),
    }
}

fn proto_contract_input_from_contract(
    contract: &fuel_core_types::fuel_tx::input::contract::Contract,
) -> ProtoContractInput {
    ProtoContractInput {
        utxo_id: Some(proto_utxo_id_from_utxo_id(&contract.utxo_id)),
        balance_root: bytes32_to_vec(&contract.balance_root),
        state_root: bytes32_to_vec(&contract.state_root),
        tx_pointer: Some(proto_tx_pointer(&contract.tx_pointer)),
        contract_id: contract.contract_id.as_ref().to_vec(),
    }
}

fn proto_contract_output_from_contract(
    contract: &fuel_core_types::fuel_tx::output::contract::Contract,
) -> ProtoContractOutput {
    ProtoContractOutput {
        input_index: u32::from(contract.input_index),
        balance_root: bytes32_to_vec(&contract.balance_root),
        state_root: bytes32_to_vec(&contract.state_root),
    }
}

fn proto_output_from_output(output: Output) -> ProtoOutput {
    let variant = match output {
        Output::Coin {
            to,
            amount,
            asset_id,
        } => ProtoOutputVariant::Coin(ProtoCoinOutput {
            to: to.as_ref().to_vec(),
            amount,
            asset_id: asset_id.as_ref().to_vec(),
        }),
        Output::Contract(contract) => {
            ProtoOutputVariant::Contract(proto_contract_output_from_contract(&contract))
        }
        Output::Change {
            to,
            amount,
            asset_id,
        } => ProtoOutputVariant::Change(ProtoChangeOutput {
            to: to.as_ref().to_vec(),
            amount,
            asset_id: asset_id.as_ref().to_vec(),
        }),
        Output::Variable {
            to,
            amount,
            asset_id,
        } => ProtoOutputVariant::Variable(ProtoVariableOutput {
            to: to.as_ref().to_vec(),
            amount,
            asset_id: asset_id.as_ref().to_vec(),
        }),
        Output::ContractCreated {
            contract_id,
            state_root,
        } => ProtoOutputVariant::ContractCreated(ProtoContractCreatedOutput {
            contract_id: contract_id.as_ref().to_vec(),
            state_root: bytes32_to_vec(&state_root),
        }),
    };

    ProtoOutput {
        variant: Some(variant),
    }
}

fn proto_upgrade_purpose(purpose: &UpgradePurpose) -> ProtoUpgradePurpose {
    let variant = match purpose {
        UpgradePurpose::ConsensusParameters {
            witness_index,
            checksum,
        } => ProtoUpgradePurposeVariant::ConsensusParameters(
            ProtoUpgradeConsensusParameters {
                witness_index: u32::from(*witness_index),
                checksum: checksum.as_ref().to_vec(),
            },
        ),
        UpgradePurpose::StateTransition { root } => {
            ProtoUpgradePurposeVariant::StateTransition(ProtoUpgradeStateTransition {
                root: root.as_ref().to_vec(),
            })
        }
    };

    ProtoUpgradePurpose {
        variant: Some(variant),
    }
}

fn proto_policies_from_policies(
    policies: &fuel_core_types::fuel_tx::policies::Policies,
) -> ProtoPolicies {
    let mut values = [0u64; 6];
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
    if policies.is_set(PolicyType::Owner) {
        values[5] = policies.get(PolicyType::Owner).unwrap_or_default();
    }
    let bits = policies.bits();
    ProtoPolicies {
        bits,
        values: values.to_vec(),
    }
}

fn tx_pointer_from_proto(proto: &ProtoTxPointer) -> Result<TxPointer> {
    let block_height = proto.block_height.into();
    #[allow(clippy::useless_conversion)]
    let tx_index = proto.tx_index.try_into().map_err(|e| {
        Error::Serialization(anyhow!("Could not convert tx_index to target type: {}", e))
    })?;
    Ok(TxPointer::new(block_height, tx_index))
}

fn storage_slot_from_proto(proto: &ProtoStorageSlot) -> Result<StorageSlot> {
    let key = Bytes32::try_from(proto.key.as_slice()).map_err(|e| {
        Error::Serialization(anyhow!(
            "Could not convert storage slot key to Bytes32: {}",
            e
        ))
    })?;
    let value = Bytes32::try_from(proto.value.as_slice()).map_err(|e| {
        Error::Serialization(anyhow!(
            "Could not convert storage slot value to Bytes32: {}",
            e
        ))
    })?;
    Ok(StorageSlot::new(key, value))
}

fn contract_input_from_proto(
    proto: &ProtoContractInput,
) -> Result<fuel_core_types::fuel_tx::input::contract::Contract> {
    let utxo_proto = proto.utxo_id.as_ref().ok_or_else(|| {
        Error::Serialization(anyhow!("Missing utxo_id on contract input"))
    })?;
    let utxo_id = utxo_id_from_proto(utxo_proto)?;
    let balance_root = Bytes32::try_from(proto.balance_root.as_slice()).map_err(|e| {
        Error::Serialization(anyhow!("Could not convert balance_root to Bytes32: {}", e))
    })?;
    let state_root = Bytes32::try_from(proto.state_root.as_slice()).map_err(|e| {
        Error::Serialization(anyhow!("Could not convert state_root to Bytes32: {}", e))
    })?;
    let tx_pointer_proto = proto.tx_pointer.as_ref().ok_or_else(|| {
        Error::Serialization(anyhow!("Missing tx_pointer on contract input"))
    })?;
    let tx_pointer = tx_pointer_from_proto(tx_pointer_proto)?;
    let contract_id =
        fuel_core_types::fuel_types::ContractId::try_from(proto.contract_id.as_slice())
            .map_err(|e| Error::Serialization(anyhow!(e)))?;

    Ok(fuel_core_types::fuel_tx::input::contract::Contract {
        utxo_id,
        balance_root,
        state_root,
        tx_pointer,
        contract_id,
    })
}

fn contract_output_from_proto(
    proto: &ProtoContractOutput,
) -> Result<fuel_core_types::fuel_tx::output::contract::Contract> {
    let input_index = u16::try_from(proto.input_index).map_err(|e| {
        Error::Serialization(anyhow!(
            "Could not convert contract output input_index to u16: {}",
            e
        ))
    })?;
    let balance_root = Bytes32::try_from(proto.balance_root.as_slice()).map_err(|e| {
        Error::Serialization(anyhow!(
            "Could not convert contract output balance_root to Bytes32: {}",
            e
        ))
    })?;
    let state_root = Bytes32::try_from(proto.state_root.as_slice()).map_err(|e| {
        Error::Serialization(anyhow!(
            "Could not convert contract output state_root to Bytes32: {}",
            e
        ))
    })?;

    Ok(fuel_core_types::fuel_tx::output::contract::Contract {
        input_index,
        balance_root,
        state_root,
    })
}

fn output_from_proto_output(proto_output: &ProtoOutput) -> Result<Output> {
    match proto_output
        .variant
        .as_ref()
        .ok_or_else(|| Error::Serialization(anyhow!("Missing output variant")))?
    {
        ProtoOutputVariant::Coin(coin) => {
            let to = Address::try_from(coin.to.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let asset_id =
                fuel_core_types::fuel_types::AssetId::try_from(coin.asset_id.as_slice())
                    .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(Output::coin(to, coin.amount, asset_id))
        }
        ProtoOutputVariant::Contract(contract) => {
            let contract = contract_output_from_proto(contract)?;
            Ok(Output::Contract(contract))
        }
        ProtoOutputVariant::Change(change) => {
            let to = Address::try_from(change.to.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let asset_id = fuel_core_types::fuel_types::AssetId::try_from(
                change.asset_id.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(Output::change(to, change.amount, asset_id))
        }
        ProtoOutputVariant::Variable(variable) => {
            let to = Address::try_from(variable.to.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let asset_id = fuel_core_types::fuel_types::AssetId::try_from(
                variable.asset_id.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(Output::variable(to, variable.amount, asset_id))
        }
        ProtoOutputVariant::ContractCreated(contract_created) => {
            let contract_id = fuel_core_types::fuel_types::ContractId::try_from(
                contract_created.contract_id.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let state_root = Bytes32::try_from(contract_created.state_root.as_slice())
                .map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert state_root to Bytes32: {}",
                        e
                    ))
                })?;
            Ok(Output::contract_created(contract_id, state_root))
        }
    }
}

fn upgrade_purpose_from_proto(proto: &ProtoUpgradePurpose) -> Result<UpgradePurpose> {
    match proto
        .variant
        .as_ref()
        .ok_or_else(|| Error::Serialization(anyhow!("Missing upgrade purpose variant")))?
    {
        ProtoUpgradePurposeVariant::ConsensusParameters(consensus) => {
            let witness_index = u16::try_from(consensus.witness_index).map_err(|e| {
                Error::Serialization(anyhow!(
                    "Could not convert witness_index to u16: {}",
                    e
                ))
            })?;
            let checksum =
                Bytes32::try_from(consensus.checksum.as_slice()).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert checksum to Bytes32: {}",
                        e
                    ))
                })?;
            Ok(UpgradePurpose::ConsensusParameters {
                witness_index,
                checksum,
            })
        }
        ProtoUpgradePurposeVariant::StateTransition(state) => {
            let root = Bytes32::try_from(state.root.as_slice()).map_err(|e| {
                Error::Serialization(anyhow!(
                    "Could not convert state transition root to Bytes32: {}",
                    e
                ))
            })?;
            Ok(UpgradePurpose::StateTransition { root })
        }
    }
}

fn utxo_id_from_proto(proto_utxo: &ProtoUtxoId) -> Result<UtxoId> {
    let tx_id = Bytes32::try_from(proto_utxo.tx_id.as_slice()).map_err(|e| {
        Error::Serialization(anyhow!("Could not convert tx_id to Bytes32: {}", e))
    })?;
    let output_index = u16::try_from(proto_utxo.output_index).map_err(|e| {
        Error::Serialization(anyhow!("Could not convert output_index to u16: {}", e))
    })?;
    Ok(UtxoId::new(tx_id, output_index))
}

fn bytes32_to_vec(bytes: &fuel_core_types::fuel_types::Bytes32) -> Vec<u8> {
    bytes.as_ref().to_vec()
}

pub fn fuel_block_from_protobuf(
    proto_block: ProtoBlock,
    msg_ids: &[fuel_core_types::fuel_tx::MessageId],
    event_inbox_root: Bytes32,
) -> Result<FuelBlock> {
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
        msg_ids,
        event_inbox_root,
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
    let variant = proto_tx
        .variant
        .as_ref()
        .ok_or_else(|| Error::Serialization(anyhow!("Missing transaction variant")))?;

    match variant {
        ProtoTransactionVariant::Script(proto_script) => {
            let policies = proto_script
                .policies
                .clone()
                .map(policies_from_proto_policies)
                .unwrap_or_default();
            let inputs = proto_script
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<Result<Vec<_>>>()?;
            let outputs = proto_script
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<Result<Vec<_>>>()?;
            let witnesses = proto_script
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<Result<Vec<_>>>()?;
            let mut script_tx = FuelTransaction::script(
                proto_script.script_gas_limit,
                proto_script.script.clone(),
                proto_script.script_data.clone(),
                policies,
                inputs,
                outputs,
                witnesses,
            );
            *script_tx.receipts_root_mut() = Bytes32::try_from(
                proto_script.receipts_root.as_slice(),
            )
            .map_err(|e| {
                Error::Serialization(anyhow!(
                    "Could not convert receipts_root to Bytes32: {}",
                    e
                ))
            })?;

            Ok(FuelTransaction::Script(script_tx))
        }
        ProtoTransactionVariant::Create(proto_create) => {
            let policies = proto_create
                .policies
                .clone()
                .map(policies_from_proto_policies)
                .unwrap_or_default();
            let inputs = proto_create
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<Result<Vec<_>>>()?;
            let outputs = proto_create
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<Result<Vec<_>>>()?;
            let witnesses = proto_create
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<Result<Vec<_>>>()?;
            let storage_slots = proto_create
                .storage_slots
                .iter()
                .map(storage_slot_from_proto)
                .collect::<Result<Vec<_>>>()?;
            let salt =
                fuel_core_types::fuel_types::Salt::try_from(proto_create.salt.as_slice())
                    .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let bytecode_witness_index =
                u16::try_from(proto_create.bytecode_witness_index).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert bytecode_witness_index to u16: {}",
                        e
                    ))
                })?;

            let create_tx = FuelTransaction::create(
                bytecode_witness_index,
                policies,
                salt,
                storage_slots,
                inputs,
                outputs,
                witnesses,
            );

            Ok(FuelTransaction::Create(create_tx))
        }
        ProtoTransactionVariant::Mint(proto_mint) => {
            let tx_pointer_proto = proto_mint.tx_pointer.as_ref().ok_or_else(|| {
                Error::Serialization(anyhow!("Missing tx_pointer on mint transaction"))
            })?;
            let tx_pointer = tx_pointer_from_proto(tx_pointer_proto)?;
            let input_contract_proto =
                proto_mint.input_contract.as_ref().ok_or_else(|| {
                    Error::Serialization(anyhow!(
                        "Missing input_contract on mint transaction"
                    ))
                })?;
            let input_contract = contract_input_from_proto(input_contract_proto)?;
            let output_contract_proto =
                proto_mint.output_contract.as_ref().ok_or_else(|| {
                    Error::Serialization(anyhow!(
                        "Missing output_contract on mint transaction"
                    ))
                })?;
            let output_contract = contract_output_from_proto(output_contract_proto)?;
            let mint_asset_id = fuel_core_types::fuel_types::AssetId::try_from(
                proto_mint.mint_asset_id.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;

            let mint_tx = FuelTransaction::mint(
                tx_pointer,
                input_contract,
                output_contract,
                proto_mint.mint_amount,
                mint_asset_id,
                proto_mint.gas_price,
            );

            Ok(FuelTransaction::Mint(mint_tx))
        }
        ProtoTransactionVariant::Upgrade(proto_upgrade) => {
            let purpose_proto = proto_upgrade.purpose.as_ref().ok_or_else(|| {
                Error::Serialization(anyhow!("Missing purpose on upgrade transaction"))
            })?;
            let upgrade_purpose = upgrade_purpose_from_proto(purpose_proto)?;
            let policies = proto_upgrade
                .policies
                .clone()
                .map(policies_from_proto_policies)
                .unwrap_or_default();
            let inputs = proto_upgrade
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<Result<Vec<_>>>()?;
            let outputs = proto_upgrade
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<Result<Vec<_>>>()?;
            let witnesses = proto_upgrade
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<Result<Vec<_>>>()?;

            let upgrade_tx = FuelTransaction::upgrade(
                upgrade_purpose,
                policies,
                inputs,
                outputs,
                witnesses,
            );

            Ok(FuelTransaction::Upgrade(upgrade_tx))
        }
        ProtoTransactionVariant::Upload(proto_upload) => {
            let policies = proto_upload
                .policies
                .clone()
                .map(policies_from_proto_policies)
                .unwrap_or_default();
            let inputs = proto_upload
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<Result<Vec<_>>>()?;
            let outputs = proto_upload
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<Result<Vec<_>>>()?;
            let witnesses = proto_upload
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<Result<Vec<_>>>()?;
            let root = Bytes32::try_from(proto_upload.root.as_slice()).map_err(|e| {
                Error::Serialization(anyhow!(
                    "Could not convert upload root to Bytes32: {}",
                    e
                ))
            })?;
            let witness_index =
                u16::try_from(proto_upload.witness_index).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert witness_index to u16: {}",
                        e
                    ))
                })?;
            let subsection_index =
                u16::try_from(proto_upload.subsection_index).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert subsection_index to u16: {}",
                        e
                    ))
                })?;
            let subsections_number = u16::try_from(proto_upload.subsections_number)
                .map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert subsections_number to u16: {}",
                        e
                    ))
                })?;
            let proof_set = proto_upload
                .proof_set
                .iter()
                .map(|entry| {
                    Bytes32::try_from(entry.as_slice()).map_err(|e| {
                        Error::Serialization(anyhow!(
                            "Could not convert proof_set entry to Bytes32: {}",
                            e
                        ))
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            let body = UploadBody {
                root,
                witness_index,
                subsection_index,
                subsections_number,
                proof_set,
            };

            let upload_tx =
                FuelTransaction::upload(body, policies, inputs, outputs, witnesses);

            Ok(FuelTransaction::Upload(upload_tx))
        }
        ProtoTransactionVariant::Blob(proto_blob) => {
            let policies = proto_blob
                .policies
                .clone()
                .map(policies_from_proto_policies)
                .unwrap_or_default();
            let inputs = proto_blob
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<Result<Vec<_>>>()?;
            let outputs = proto_blob
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<Result<Vec<_>>>()?;
            let witnesses = proto_blob
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<Result<Vec<_>>>()?;
            let blob_id = fuel_core_types::fuel_types::BlobId::try_from(
                proto_blob.blob_id.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let witness_index = u16::try_from(proto_blob.witness_index).map_err(|e| {
                Error::Serialization(anyhow!(
                    "Could not convert blob witness_index to u16: {}",
                    e
                ))
            })?;
            let body = BlobBody {
                id: blob_id,
                witness_index,
            };

            let blob_tx =
                FuelTransaction::blob(body, policies, inputs, outputs, witnesses);

            Ok(FuelTransaction::Blob(blob_tx))
        }
    }
}

fn input_from_proto_input(proto_input: &ProtoInput) -> Result<Input> {
    let variant = proto_input
        .variant
        .as_ref()
        .ok_or_else(|| Error::Serialization(anyhow!("Missing input variant")))?;

    match variant {
        ProtoInputVariant::CoinSigned(proto_coin_signed) => {
            let utxo_proto = proto_coin_signed
                .utxo_id
                .as_ref()
                .ok_or_else(|| Error::Serialization(anyhow!("Missing utxo_id")))?;
            let utxo_id = utxo_id_from_proto(utxo_proto)?;
            let owner =
                Address::try_from(proto_coin_signed.owner.as_slice()).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert owner to Address: {}",
                        e
                    ))
                })?;
            let asset_id = fuel_core_types::fuel_types::AssetId::try_from(
                proto_coin_signed.asset_id.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let tx_pointer_proto = proto_coin_signed
                .tx_pointer
                .as_ref()
                .ok_or_else(|| Error::Serialization(anyhow!("Missing tx_pointer")))?;
            let tx_pointer = tx_pointer_from_proto(tx_pointer_proto)?;
            let witness_index =
                u16::try_from(proto_coin_signed.witness_index).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert witness_index to u16: {}",
                        e
                    ))
                })?;

            Ok(Input::coin_signed(
                utxo_id,
                owner,
                proto_coin_signed.amount,
                asset_id,
                tx_pointer,
                witness_index,
            ))
        }
        ProtoInputVariant::CoinPredicate(proto_coin_predicate) => {
            let utxo_proto = proto_coin_predicate
                .utxo_id
                .as_ref()
                .ok_or_else(|| Error::Serialization(anyhow!("Missing utxo_id")))?;
            let utxo_id = utxo_id_from_proto(utxo_proto)?;
            let owner = Address::try_from(proto_coin_predicate.owner.as_slice())
                .map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert owner to Address: {}",
                        e
                    ))
                })?;
            let asset_id = fuel_core_types::fuel_types::AssetId::try_from(
                proto_coin_predicate.asset_id.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let tx_pointer_proto = proto_coin_predicate
                .tx_pointer
                .as_ref()
                .ok_or_else(|| Error::Serialization(anyhow!("Missing tx_pointer")))?;
            let tx_pointer = tx_pointer_from_proto(tx_pointer_proto)?;

            Ok(Input::coin_predicate(
                utxo_id,
                owner,
                proto_coin_predicate.amount,
                asset_id,
                tx_pointer,
                proto_coin_predicate.predicate_gas_used,
                proto_coin_predicate.predicate.clone(),
                proto_coin_predicate.predicate_data.clone(),
            ))
        }
        ProtoInputVariant::Contract(proto_contract) => {
            let contract = contract_input_from_proto(proto_contract)?;
            Ok(Input::Contract(contract))
        }
        ProtoInputVariant::MessageCoinSigned(proto_message) => {
            let sender =
                Address::try_from(proto_message.sender.as_slice()).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert sender to Address: {}",
                        e
                    ))
                })?;
            let recipient = Address::try_from(proto_message.recipient.as_slice())
                .map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert recipient to Address: {}",
                        e
                    ))
                })?;
            let nonce = fuel_core_types::fuel_types::Nonce::try_from(
                proto_message.nonce.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let witness_index =
                u16::try_from(proto_message.witness_index).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert witness_index to u16: {}",
                        e
                    ))
                })?;

            Ok(Input::message_coin_signed(
                sender,
                recipient,
                proto_message.amount,
                nonce,
                witness_index,
            ))
        }
        ProtoInputVariant::MessageCoinPredicate(proto_message) => {
            let sender =
                Address::try_from(proto_message.sender.as_slice()).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert sender to Address: {}",
                        e
                    ))
                })?;
            let recipient = Address::try_from(proto_message.recipient.as_slice())
                .map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert recipient to Address: {}",
                        e
                    ))
                })?;
            let nonce = fuel_core_types::fuel_types::Nonce::try_from(
                proto_message.nonce.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;

            Ok(Input::message_coin_predicate(
                sender,
                recipient,
                proto_message.amount,
                nonce,
                proto_message.predicate_gas_used,
                proto_message.predicate.clone(),
                proto_message.predicate_data.clone(),
            ))
        }
        ProtoInputVariant::MessageDataSigned(proto_message) => {
            let sender =
                Address::try_from(proto_message.sender.as_slice()).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert sender to Address: {}",
                        e
                    ))
                })?;
            let recipient = Address::try_from(proto_message.recipient.as_slice())
                .map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert recipient to Address: {}",
                        e
                    ))
                })?;
            let nonce = fuel_core_types::fuel_types::Nonce::try_from(
                proto_message.nonce.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let witness_index =
                u16::try_from(proto_message.witness_index).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert witness_index to u16: {}",
                        e
                    ))
                })?;

            Ok(Input::message_data_signed(
                sender,
                recipient,
                proto_message.amount,
                nonce,
                witness_index,
                proto_message.data.clone(),
            ))
        }
        ProtoInputVariant::MessageDataPredicate(proto_message) => {
            let sender =
                Address::try_from(proto_message.sender.as_slice()).map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert sender to Address: {}",
                        e
                    ))
                })?;
            let recipient = Address::try_from(proto_message.recipient.as_slice())
                .map_err(|e| {
                    Error::Serialization(anyhow!(
                        "Could not convert recipient to Address: {}",
                        e
                    ))
                })?;
            let nonce = fuel_core_types::fuel_types::Nonce::try_from(
                proto_message.nonce.as_slice(),
            )
            .map_err(|e| Error::Serialization(anyhow!(e)))?;

            Ok(Input::message_data_predicate(
                sender,
                recipient,
                proto_message.amount,
                nonce,
                proto_message.predicate_gas_used,
                proto_message.data.clone(),
                proto_message.predicate.clone(),
                proto_message.predicate_data.clone(),
            ))
        }
    }
}

fn policies_from_proto_policies(proto_policies: ProtoPolicies) -> FuelPolicies {
    let ProtoPolicies { bits, values } = proto_policies;
    let mut policies = FuelPolicies::default();
    let bits =
        PoliciesBits::from_bits(bits).expect("Should be able to create from `u32`");
    if bits.contains(PoliciesBits::Tip)
        && let Some(tip) = values.get(0)
    {
        policies.set(PolicyType::Tip, Some(*tip));
    }
    if bits.contains(PoliciesBits::WitnessLimit)
        && let Some(witness_limit) = values.get(1)
    {
        policies.set(PolicyType::WitnessLimit, Some(*witness_limit));
    }
    if bits.contains(PoliciesBits::Maturity)
        && let Some(maturity) = values.get(2)
    {
        policies.set(PolicyType::Maturity, Some(*maturity));
    }
    if bits.contains(PoliciesBits::MaxFee)
        && let Some(max_fee) = values.get(3)
    {
        policies.set(PolicyType::MaxFee, Some(*max_fee));
    }
    if bits.contains(PoliciesBits::Expiration)
        && let Some(expiration) = values.get(4)
    {
        policies.set(PolicyType::Expiration, Some(*expiration));
    }
    if bits.contains(PoliciesBits::Owner)
        && let Some(owner) = values.get(5)
    {
        policies.set(PolicyType::Owner, Some(*owner));
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
    use fuel_core_types::test_helpers::arb_block;
    use proptest::prelude::*;

    proptest! {
            #![proptest_config(ProptestConfig {
      cases: 100, .. ProptestConfig::default()
    })]
          #[test]
          fn serialize_block__roundtrip((block, msg_ids, event_inbox_root) in arb_block()) {
              // given
              let serializer = SerializerAdapter;

              // when
              let proto_block = serializer.serialize_block(&block).unwrap();

              // then
              let deserialized_block = fuel_block_from_protobuf(proto_block, &msg_ids, event_inbox_root).unwrap();
              assert_eq!(block, deserialized_block);

      }
      }

    #[test]
    #[ignore]
    fn _dummy() {}
}
