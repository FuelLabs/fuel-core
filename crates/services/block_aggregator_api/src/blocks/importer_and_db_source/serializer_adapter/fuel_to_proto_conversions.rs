#[cfg(feature = "fault-proving")]
use crate::protobuf_types::V2Header as ProtoV2Header;
use crate::{
    blocks::importer_and_db_source::serializer_adapter::proto_to_fuel_conversions::bytes32_to_vec,
    protobuf_types::{
        BlobTransaction as ProtoBlobTx,
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
        V1Header as ProtoV1Header,
        VariableOutput as ProtoVariableOutput,
        header::VersionedHeader as ProtoVersionedHeader,
        input::Variant as ProtoInputVariant,
        output::Variant as ProtoOutputVariant,
        transaction::Variant as ProtoTransactionVariant,
        upgrade_purpose::Variant as ProtoUpgradePurposeVariant,
    },
};

use fuel_core_types::{
    blockchain::{
        header::{
            BlockHeader,
            BlockHeaderV1,
            ConsensusHeader,
            GeneratedConsensusFields,
        },
        primitives::BlockId,
    },
    fuel_tx::{
        Input,
        Output,
        StorageSlot,
        Transaction as FuelTransaction,
        TxPointer,
        UpgradePurpose,
        UtxoId,
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
        policies::PolicyType,
    },
};

pub fn proto_header_from_header(header: &BlockHeader) -> ProtoHeader {
    let block_id = header.id();
    let consensus = header.consensus();
    let versioned_header = match header {
        BlockHeader::V1(header) => {
            let proto_v1_header =
                proto_v1_header_from_v1_header(&consensus, &block_id, header);
            ProtoVersionedHeader::V1(proto_v1_header)
        }
        #[cfg(feature = "fault-proving")]
        BlockHeader::V2(header) => {
            let proto_v2_header =
                proto_v2_header_from_v2_header(consensus, &block_id, header);
            ProtoVersionedHeader::V2(proto_v2_header)
        }
    };

    ProtoHeader {
        versioned_header: Some(versioned_header),
    }
}

fn proto_v1_header_from_v1_header(
    consensus: &ConsensusHeader<GeneratedConsensusFields>,
    block_id: &BlockId,
    header: &BlockHeaderV1,
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
    consensus: &ConsensusHeader<GeneratedConsensusFields>,
    block_id: &BlockId,
    header: &BlockHeaderV2,
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

pub fn proto_tx_from_tx(tx: &FuelTransaction) -> ProtoTransaction {
    match tx {
        FuelTransaction::Script(script) => {
            let proto_script = ProtoScriptTx {
                script_gas_limit: *script.script_gas_limit(),
                receipts_root: bytes32_to_vec(script.receipts_root()),
                script: script.script().clone(),
                script_data: script.script_data().clone(),
                policies: Some(proto_policies_from_policies(script.policies())),
                inputs: script.inputs().iter().map(proto_input_from_input).collect(),
                outputs: script
                    .outputs()
                    .iter()
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
                inputs: create.inputs().iter().map(proto_input_from_input).collect(),
                outputs: create
                    .outputs()
                    .iter()
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
                    .map(proto_input_from_input)
                    .collect(),
                outputs: upgrade
                    .outputs()
                    .iter()
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
                inputs: upload.inputs().iter().map(proto_input_from_input).collect(),
                outputs: upload
                    .outputs()
                    .iter()
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
                inputs: blob.inputs().iter().map(proto_input_from_input).collect(),
                outputs: blob
                    .outputs()
                    .iter()
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

fn proto_input_from_input(input: &Input) -> ProtoInput {
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

fn proto_output_from_output(output: &Output) -> ProtoOutput {
    let variant = match output {
        Output::Coin {
            to,
            amount,
            asset_id,
        } => ProtoOutputVariant::Coin(ProtoCoinOutput {
            to: to.as_ref().to_vec(),
            amount: *amount,
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
            amount: *amount,
            asset_id: asset_id.as_ref().to_vec(),
        }),
        Output::Variable {
            to,
            amount,
            asset_id,
        } => ProtoOutputVariant::Variable(ProtoVariableOutput {
            to: to.as_ref().to_vec(),
            amount: *amount,
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
    let mut truncated_len = 0;
    if let Some(value) = policies.get(PolicyType::Tip) {
        values[0] = value;
        truncated_len = 1;
    }
    if let Some(value) = policies.get(PolicyType::WitnessLimit) {
        values[1] = value;
        truncated_len = 2;
    }
    if let Some(value) = policies.get(PolicyType::Maturity) {
        values[2] = value;
        truncated_len = 3;
    }
    if let Some(value) = policies.get(PolicyType::MaxFee) {
        values[3] = value;
        truncated_len = 4;
    }
    if let Some(value) = policies.get(PolicyType::Expiration) {
        values[4] = value;
        truncated_len = 5;
    }
    if let Some(value) = policies.get(PolicyType::Owner) {
        values[5] = value;
        truncated_len = 6;
    }
    let bits = policies.bits();
    values[..truncated_len].to_vec();
    ProtoPolicies {
        bits,
        values: values.to_vec(),
    }
}
