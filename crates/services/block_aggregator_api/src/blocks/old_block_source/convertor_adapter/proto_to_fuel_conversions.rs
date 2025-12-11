#[cfg(feature = "fault-proving")]
use crate::blocks::old_block_source::convertor_adapter::ChainId;
use crate::{
    protobuf_types::{
        Block as ProtoBlock,
        ContractInput as ProtoContractInput,
        ContractOutput as ProtoContractOutput,
        Header as ProtoHeader,
        Input as ProtoInput,
        Output as ProtoOutput,
        PanicInstruction as ProtoPanicInstruction,
        Policies as ProtoPolicies,
        StorageSlot as ProtoStorageSlot,
        Transaction as ProtoTransaction,
        TxPointer as ProtoTxPointer,
        UpgradePurpose as ProtoUpgradePurpose,
        UtxoId as ProtoUtxoId,
        block::VersionedBlock as ProtoVersionedBlock,
        header::VersionedHeader as ProtoVersionedHeader,
        input::Variant as ProtoInputVariant,
        output::Variant as ProtoOutputVariant,
        receipt::Variant as ProtoReceiptVariant,
        script_execution_result::Variant as ProtoScriptExecutionResultVariant,
        transaction::Variant as ProtoTransactionVariant,
        upgrade_purpose::Variant as ProtoUpgradePurposeVariant,
    },
    result::Error,
};
use anyhow::anyhow;
use fuel_core_types::{
    blockchain::{
        block::Block as FuelBlock,
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::{
            DaBlockHeight,
            Empty,
        },
    },
    fuel_asm::{
        PanicInstruction,
        PanicReason,
    },
    fuel_tx::{
        Address,
        BlobBody,
        Bytes32,
        Input,
        Output,
        Receipt as FuelReceipt,
        ScriptExecutionResult,
        StorageSlot,
        Transaction as FuelTransaction,
        TxPointer,
        UpgradePurpose,
        UploadBody,
        UtxoId,
        Witness,
        field::ReceiptsRoot as _,
        policies::{
            Policies as FuelPolicies,
            PoliciesBits,
            PolicyType,
        },
    },
    fuel_types::{
        AssetId,
        ContractId,
        Nonce,
        SubAssetId,
    },
    tai64,
};

fn tx_pointer_from_proto(proto: &ProtoTxPointer) -> crate::result::Result<TxPointer> {
    let block_height = proto.block_height.into();
    #[allow(clippy::useless_conversion)]
    let tx_index = proto.tx_index.try_into().map_err(|e| {
        Error::Serialization(anyhow!("Could not convert tx_index to target type: {}", e))
    })?;
    Ok(TxPointer::new(block_height, tx_index))
}

fn storage_slot_from_proto(
    proto: &ProtoStorageSlot,
) -> crate::result::Result<StorageSlot> {
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
) -> crate::result::Result<fuel_core_types::fuel_tx::input::contract::Contract> {
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
) -> crate::result::Result<fuel_core_types::fuel_tx::output::contract::Contract> {
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

fn output_from_proto_output(proto_output: &ProtoOutput) -> crate::result::Result<Output> {
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

fn upgrade_purpose_from_proto(
    proto: &ProtoUpgradePurpose,
) -> crate::result::Result<UpgradePurpose> {
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

fn utxo_id_from_proto(proto_utxo: &ProtoUtxoId) -> crate::result::Result<UtxoId> {
    let tx_id = Bytes32::try_from(proto_utxo.tx_id.as_slice()).map_err(|e| {
        Error::Serialization(anyhow!("Could not convert tx_id to Bytes32: {}", e))
    })?;
    let output_index = u16::try_from(proto_utxo.output_index).map_err(|e| {
        Error::Serialization(anyhow!("Could not convert output_index to u16: {}", e))
    })?;
    Ok(UtxoId::new(tx_id, output_index))
}

pub fn bytes32_to_vec(bytes: &fuel_core_types::fuel_types::Bytes32) -> Vec<u8> {
    bytes.as_ref().to_vec()
}

fn script_execution_result_from_proto(
    proto: &crate::protobuf_types::ScriptExecutionResult,
) -> crate::result::Result<ScriptExecutionResult> {
    let variant = proto.variant.as_ref().ok_or_else(|| {
        Error::Serialization(anyhow!("Missing script execution result variant"))
    })?;

    let result = match variant {
        ProtoScriptExecutionResultVariant::Success(_) => ScriptExecutionResult::Success,
        ProtoScriptExecutionResultVariant::Revert(_) => ScriptExecutionResult::Revert,
        ProtoScriptExecutionResultVariant::Panic(_) => ScriptExecutionResult::Panic,
        ProtoScriptExecutionResultVariant::GenericFailure(failure) => {
            ScriptExecutionResult::GenericFailure(failure.code)
        }
    };

    Ok(result)
}

fn panic_instruction_from_proto(proto: &ProtoPanicInstruction) -> PanicInstruction {
    use crate::protobuf_types::PanicReason as ProtoPanicReason;

    let reason_proto =
        ProtoPanicReason::try_from(proto.reason).unwrap_or(ProtoPanicReason::Unknown);
    let reason = PanicReason::from(reason_proto as u8);
    PanicInstruction::error(reason, proto.instruction)
}

fn receipt_from_proto(
    proto_receipt: &crate::protobuf_types::Receipt,
) -> crate::result::Result<FuelReceipt> {
    let variant = proto_receipt
        .variant
        .as_ref()
        .ok_or_else(|| Error::Serialization(anyhow!("Missing receipt variant")))?;

    let receipt = match variant {
        ProtoReceiptVariant::Call(call) => {
            let id = ContractId::try_from(call.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let to = ContractId::try_from(call.to.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let asset_id = AssetId::try_from(call.asset_id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(FuelReceipt::call(
                id,
                to,
                call.amount,
                asset_id,
                call.gas,
                call.param1,
                call.param2,
                call.pc,
                call.is,
            ))
        }
        ProtoReceiptVariant::ReturnReceipt(ret) => {
            let id = ContractId::try_from(ret.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(FuelReceipt::ret(id, ret.val, ret.pc, ret.is))
        }
        ProtoReceiptVariant::ReturnData(rd) => {
            let id = ContractId::try_from(rd.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let digest = Bytes32::try_from(rd.digest.as_slice()).map_err(|e| {
                Error::Serialization(anyhow!(
                    "Could not convert return data digest to Bytes32: {}",
                    e
                ))
            })?;
            Ok(FuelReceipt::return_data_with_len(
                id,
                rd.ptr,
                rd.len,
                digest,
                rd.pc,
                rd.is,
                rd.data.clone(),
            ))
        }
        ProtoReceiptVariant::Panic(panic_receipt) => {
            let id = ContractId::try_from(panic_receipt.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let reason_proto = panic_receipt
                .reason
                .as_ref()
                .ok_or_else(|| Error::Serialization(anyhow!("Missing panic reason")))?;
            let reason = panic_instruction_from_proto(reason_proto);
            let contract_id = panic_receipt
                .contract_id
                .as_ref()
                .map(|cid| {
                    ContractId::try_from(cid.as_slice())
                        .map_err(|e| Error::Serialization(anyhow!(e)))
                })
                .transpose()?;
            Ok(
                FuelReceipt::panic(id, reason, panic_receipt.pc, panic_receipt.is)
                    .with_panic_contract_id(contract_id),
            )
        }
        ProtoReceiptVariant::Revert(revert) => {
            let id = ContractId::try_from(revert.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(FuelReceipt::revert(id, revert.ra, revert.pc, revert.is))
        }
        ProtoReceiptVariant::Log(log) => {
            let id = ContractId::try_from(log.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(FuelReceipt::log(
                id, log.ra, log.rb, log.rc, log.rd, log.pc, log.is,
            ))
        }
        ProtoReceiptVariant::LogData(log) => {
            let id = ContractId::try_from(log.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let digest = Bytes32::try_from(log.digest.as_slice()).map_err(|e| {
                Error::Serialization(anyhow!(
                    "Could not convert log data digest to Bytes32: {}",
                    e
                ))
            })?;
            Ok(FuelReceipt::log_data_with_len(
                id,
                log.ra,
                log.rb,
                log.ptr,
                log.len,
                digest,
                log.pc,
                log.is,
                log.data.clone(),
            ))
        }
        ProtoReceiptVariant::Transfer(transfer) => {
            let id = ContractId::try_from(transfer.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let to = ContractId::try_from(transfer.to.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let asset_id = AssetId::try_from(transfer.asset_id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(FuelReceipt::transfer(
                id,
                to,
                transfer.amount,
                asset_id,
                transfer.pc,
                transfer.is,
            ))
        }
        ProtoReceiptVariant::TransferOut(transfer) => {
            let id = ContractId::try_from(transfer.id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let to = Address::try_from(transfer.to.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let asset_id = AssetId::try_from(transfer.asset_id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(FuelReceipt::transfer_out(
                id,
                to,
                transfer.amount,
                asset_id,
                transfer.pc,
                transfer.is,
            ))
        }
        ProtoReceiptVariant::ScriptResult(result) => {
            let script_result = result.result.as_ref().ok_or_else(|| {
                Error::Serialization(anyhow!("Missing script result payload"))
            })?;
            let execution_result = script_execution_result_from_proto(script_result)?;
            Ok(FuelReceipt::script_result(
                execution_result,
                result.gas_used,
            ))
        }
        ProtoReceiptVariant::MessageOut(msg) => {
            let sender = Address::try_from(msg.sender.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let recipient = Address::try_from(msg.recipient.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let nonce = Nonce::try_from(msg.nonce.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let digest = Bytes32::try_from(msg.digest.as_slice()).map_err(|e| {
                Error::Serialization(anyhow!(
                    "Could not convert message digest to Bytes32: {}",
                    e
                ))
            })?;
            Ok(FuelReceipt::message_out_with_len(
                sender,
                recipient,
                msg.amount,
                nonce,
                msg.len,
                digest,
                msg.data.clone(),
            ))
        }
        ProtoReceiptVariant::Mint(mint) => {
            let sub_id = SubAssetId::try_from(mint.sub_id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let contract_id = ContractId::try_from(mint.contract_id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(FuelReceipt::mint(
                sub_id,
                contract_id,
                mint.val,
                mint.pc,
                mint.is,
            ))
        }
        ProtoReceiptVariant::Burn(burn) => {
            let sub_id = SubAssetId::try_from(burn.sub_id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            let contract_id = ContractId::try_from(burn.contract_id.as_slice())
                .map_err(|e| Error::Serialization(anyhow!(e)))?;
            Ok(FuelReceipt::burn(
                sub_id,
                contract_id,
                burn.val,
                burn.pc,
                burn.is,
            ))
        }
    }?;

    Ok(receipt)
}

pub fn fuel_block_from_protobuf(
    proto_block: ProtoBlock,
    msg_ids: &[fuel_core_types::fuel_tx::MessageId],
    event_inbox_root: Bytes32,
) -> crate::result::Result<(FuelBlock, Vec<Vec<FuelReceipt>>)> {
    let versioned_block = proto_block
        .versioned_block
        .ok_or_else(|| anyhow::anyhow!("Missing protobuf versioned_block"))
        .map_err(Error::Serialization)?;
    let (partial_header, txs, receipts) = match versioned_block {
        ProtoVersionedBlock::V1(v1_inner) => {
            let proto_header = v1_inner
                .header
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Missing protobuf header"))
                .map_err(Error::Serialization)?;
            let partial_header = partial_header_from_proto_header(&proto_header)?;
            let txs = v1_inner
                .transactions
                .iter()
                .map(tx_from_proto_tx)
                .collect::<crate::result::Result<_>>()?;
            // TODO: It should be `Vec<Vec<Receipts>>`, but we need to update protobuf
            let receipts = v1_inner
                .receipts
                .iter()
                .map(|rs| {
                    rs.receipts
                        .iter()
                        .map(receipt_from_proto)
                        .collect::<crate::result::Result<Vec<_>>>()
                })
                .collect::<crate::result::Result<Vec<_>>>()?;
            (partial_header, txs, receipts)
        }
    };
    let block = FuelBlock::new(
        partial_header,
        txs,
        msg_ids,
        event_inbox_root,
        #[cfg(feature = "fault-proving")]
        &ChainId::default(),
    )
    .map_err(|e| anyhow!(e))
    .map_err(Error::Serialization)?;
    Ok((block, receipts))
}

pub fn partial_header_from_proto_header(
    proto_header: &ProtoHeader,
) -> crate::result::Result<PartialBlockHeader> {
    let partial_header = PartialBlockHeader {
        consensus: proto_header_to_empty_consensus_header(proto_header)?,
        application: proto_header_to_empty_application_header(proto_header)?,
    };
    Ok(partial_header)
}

pub fn tx_from_proto_tx(
    proto_tx: &ProtoTransaction,
) -> crate::result::Result<FuelTransaction> {
    let variant = proto_tx
        .variant
        .as_ref()
        .ok_or_else(|| Error::Serialization(anyhow!("Missing transaction variant")))?;

    match variant {
        ProtoTransactionVariant::Script(proto_script) => {
            let policies = proto_script
                .policies
                .clone()
                .map(|p| policies_from_proto_policies(&p))
                .unwrap_or_default();
            let inputs = proto_script
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let outputs = proto_script
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let witnesses = proto_script
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<crate::result::Result<Vec<_>>>()?;
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
                .map(|p| policies_from_proto_policies(&p))
                .unwrap_or_default();
            let inputs = proto_create
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let outputs = proto_create
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let witnesses = proto_create
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<crate::result::Result<Vec<_>>>()?;
            let storage_slots = proto_create
                .storage_slots
                .iter()
                .map(storage_slot_from_proto)
                .collect::<crate::result::Result<Vec<_>>>()?;
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
                .map(|p| policies_from_proto_policies(&p))
                .unwrap_or_default();
            let inputs = proto_upgrade
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let outputs = proto_upgrade
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let witnesses = proto_upgrade
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<crate::result::Result<Vec<_>>>()?;

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
                .map(|p| policies_from_proto_policies(&p))
                .unwrap_or_default();
            let inputs = proto_upload
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let outputs = proto_upload
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let witnesses = proto_upload
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<crate::result::Result<Vec<_>>>()?;
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
                .collect::<crate::result::Result<Vec<_>>>()?;

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
                .map(|p| policies_from_proto_policies(&p))
                .unwrap_or_default();
            let inputs = proto_blob
                .inputs
                .iter()
                .map(input_from_proto_input)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let outputs = proto_blob
                .outputs
                .iter()
                .map(output_from_proto_output)
                .collect::<crate::result::Result<Vec<_>>>()?;
            let witnesses = proto_blob
                .witnesses
                .iter()
                .map(|w| Ok(Witness::from(w.clone())))
                .collect::<crate::result::Result<Vec<_>>>()?;
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

fn input_from_proto_input(proto_input: &ProtoInput) -> crate::result::Result<Input> {
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

fn policies_from_proto_policies(proto_policies: &ProtoPolicies) -> FuelPolicies {
    let ProtoPolicies { bits, values } = proto_policies;
    let mut policies = FuelPolicies::default();
    let bits =
        PoliciesBits::from_bits(*bits).expect("Should be able to create from `u32`");
    if bits.contains(PoliciesBits::Tip)
        && let Some(tip) = values.first()
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
) -> crate::result::Result<ApplicationHeader<Empty>> {
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
) -> crate::result::Result<ConsensusHeader<Empty>> {
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
