// Protobuf definitions
include!(concat!(env!("OUT_DIR"), "/sf.fuel.r#type.v1.rs"));

// Prost re-export
pub use prost;

// Type conversions
use fuel_core_types::{
    blockchain::{
        block::Block as FuelBlock,
        primitives::BlockId,
    },
    fuel_asm::PanicInstruction as FuelPanicInstruction,
    fuel_tx::{
        field::{
            BytecodeLength as _,
            BytecodeWitnessIndex as _,
            InputContract as _,
            Inputs as _,
            MintAmount as _,
            MintAssetId as _,
            OutputContract as _,
            Outputs as _,
            Policies as _,
            ReceiptsRoot as _,
            Salt as _,
            Script as _,
            ScriptData as _,
            ScriptGasLimit as _,
            StorageSlots as _,
            TxPointer as _,
            Witnesses as _,
        },
        input::{
            coin::CoinFull,
            contract::Contract as FuelInputContract,
            message::FullMessage,
        },
        output::contract::Contract as FuelOutputContract,
        policies::{
            Policies as FuelPolicies,
            PolicyType,
        },
        Create as FuelCreate,
        Input as FuelInput,
        Mint as FuelMint,
        Output as FuelOutput,
        Receipt as FuelReceipt,
        Script as FuelScript,
        StorageSlot as FuelStorageSlot,
        Transaction as FuelTransaction,
        TxId as FuelTxId,
        TxPointer as FuelTxPointer,
        UtxoId as FuelUtxoId,
    },
};
use strum::IntoEnumIterator;

/// Extra info used for constructing blocks
pub struct TxExtra {
    pub id: FuelTxId,
    pub receipts: Vec<FuelReceipt>,
}

impl From<(&FuelBlock, BlockId, &[TxExtra])> for Block {
    fn from((block, prev_id, tx_extra): (&FuelBlock, BlockId, &[TxExtra])) -> Self {
        Self {
            id: block.id().as_slice().to_owned(),
            height: **block.header().height(),
            da_height: *block.header().da_height,
            msg_receipt_count: block.header().application.message_receipt_count,
            tx_root: block
                .header()
                .application
                .transactions_root
                .as_slice()
                .to_owned(),
            msg_receipt_root: block
                .header()
                .application
                .message_receipt_root
                .as_slice()
                .to_owned(),
            prev_id: prev_id.as_slice().to_owned(),
            prev_root: block.header().consensus.prev_root.as_slice().to_owned(),
            timestamp: block.header().consensus.time.0,
            application_hash: block.header().application_hash().to_vec(),
            transactions: block
                .transactions()
                .iter()
                .zip(tx_extra)
                .map(|(tx, tx_extra)| (tx, tx_extra).into())
                .collect(),
        }
    }
}

impl From<(&FuelTransaction, &TxExtra)> for Transaction {
    fn from((tx, tx_extra): (&FuelTransaction, &TxExtra)) -> Self {
        let kind = Some(match tx {
            FuelTransaction::Script(v) => {
                transaction::Kind::Script((v, &tx_extra.receipts).into())
            }
            FuelTransaction::Create(v) => transaction::Kind::Create(v.into()),
            FuelTransaction::Mint(v) => transaction::Kind::Mint(v.into()),
        });
        Transaction {
            id: (*tx_extra.id).into(),
            kind,
        }
    }
}

impl From<(&FuelScript, &Vec<FuelReceipt>)> for Script {
    fn from((value, receipts): (&FuelScript, &Vec<FuelReceipt>)) -> Self {
        Self {
            script_gas_limit: *value.script_gas_limit(),
            script: value.script().to_vec(),
            script_data: value.script_data().to_vec(),
            policies: Some(value.policies().into()),
            inputs: value.inputs().iter().map(Into::into).collect(),
            outputs: value.outputs().iter().map(Into::into).collect(),
            witnesses: value
                .witnesses()
                .iter()
                .map(|w| w.as_vec().clone())
                .collect(),
            receipts_root: value.receipts_root().as_slice().to_owned(),
            receipts: receipts.iter().map(Into::into).collect(),
        }
    }
}

impl From<&FuelCreate> for Create {
    fn from(value: &FuelCreate) -> Self {
        Self {
            bytecode_length: *value.bytecode_length(),
            bytecode_witness_index: (*value.bytecode_witness_index()).into(),
            policies: Some(value.policies().into()),
            storage_slots: value.storage_slots().iter().map(Into::into).collect(),
            inputs: value.inputs().iter().map(Into::into).collect(),
            outputs: value.outputs().iter().map(Into::into).collect(),
            witnesses: value
                .witnesses()
                .iter()
                .map(|w| w.as_vec().clone())
                .collect(),
            salt: value.salt().as_slice().to_owned(),
        }
    }
}

impl From<&FuelMint> for Mint {
    fn from(value: &FuelMint) -> Self {
        Self {
            tx_pointer: Some((*value.tx_pointer()).into()),
            input_contract: Some(value.input_contract().into()),
            output_contract: Some(value.output_contract().into()),
            mint_amount: *value.mint_amount(),
            mint_asset_id: value.mint_asset_id().as_slice().to_owned(),
        }
    }
}

impl From<&FuelInput> for Input {
    fn from(value: &FuelInput) -> Self {
        let kind = Some(match value {
            FuelInput::CoinSigned(v) => {
                input::Kind::CoinSigned(v.clone().into_full().into())
            }
            FuelInput::CoinPredicate(v) => {
                input::Kind::CoinPredicate(v.clone().into_full().into())
            }
            FuelInput::Contract(v) => input::Kind::Contract(v.into()),
            FuelInput::MessageCoinSigned(v) => {
                input::Kind::MessageCoinSigned(v.clone().into_full().into())
            }
            FuelInput::MessageCoinPredicate(v) => {
                input::Kind::MessageCoinPredicate(v.clone().into_full().into())
            }
            FuelInput::MessageDataSigned(v) => {
                input::Kind::MessageDataSigned(v.clone().into_full().into())
            }
            FuelInput::MessageDataPredicate(v) => {
                input::Kind::MessageDataPredicate(v.clone().into_full().into())
            }
        });
        Self { kind }
    }
}

impl From<CoinFull> for Coin {
    fn from(value: CoinFull) -> Self {
        Self {
            utxo_id: Some(value.utxo_id.into()),
            owner: value.owner.as_slice().to_owned(),
            amount: value.amount,
            asset_id: value.asset_id.as_slice().to_owned(),
            tx_pointer: Some(value.tx_pointer.into()),
            witness_index: value.witness_index.into(),
            maturity: *value.maturity,
            predicate_gas_used: value.predicate_gas_used,
            predicate: value.predicate,
            predicate_data: value.predicate_data,
        }
    }
}

impl From<FullMessage> for Message {
    fn from(value: FullMessage) -> Self {
        Self {
            sender: value.sender.as_slice().to_owned(),
            recipient: value.recipient.as_slice().to_owned(),
            amount: value.amount,
            nonce: value.nonce.as_slice().to_owned(),
            witness_index: value.witness_index.into(),
            predicate_gas_used: value.predicate_gas_used,
            data: value.data,
            predicate: value.predicate,
            predicate_data: value.predicate_data,
        }
    }
}

impl From<&FuelInputContract> for InputContract {
    fn from(value: &FuelInputContract) -> Self {
        Self {
            utxo_id: Some(value.utxo_id.into()),
            balance_root: value.balance_root.as_slice().to_owned(),
            state_root: value.state_root.as_slice().to_owned(),
            tx_pointer: Some(value.tx_pointer.into()),
            contract_id: value.contract_id.as_slice().to_owned(),
        }
    }
}

impl From<&FuelOutputContract> for OutputContract {
    fn from(value: &FuelOutputContract) -> Self {
        Self {
            input_index: value.input_index.into(),
            balance_root: value.balance_root.as_slice().to_owned(),
            state_root: value.state_root.as_slice().to_owned(),
        }
    }
}

impl From<&FuelOutput> for Output {
    fn from(value: &FuelOutput) -> Self {
        let kind = Some(match value {
            FuelOutput::Coin {
                to,
                amount,
                asset_id,
            } => output::Kind::Coin(OutputCoin {
                to: to.to_vec(),
                amount: *amount,
                asset_id: asset_id.to_vec(),
            }),
            FuelOutput::Contract(v) => output::Kind::Contract(v.into()),
            FuelOutput::Change {
                to,
                amount,
                asset_id,
            } => output::Kind::Change(OutputCoin {
                to: to.to_vec(),
                amount: *amount,
                asset_id: asset_id.to_vec(),
            }),
            FuelOutput::Variable {
                to,
                amount,
                asset_id,
            } => output::Kind::Variable(OutputCoin {
                to: to.to_vec(),
                amount: *amount,
                asset_id: asset_id.to_vec(),
            }),
            FuelOutput::ContractCreated {
                contract_id,
                state_root,
            } => output::Kind::ContractCreated(OutputContractCreated {
                contract_id: contract_id.to_vec(),
                state_root: state_root.to_vec(),
            }),
        });
        Self { kind }
    }
}

impl From<&FuelPolicies> for Policies {
    fn from(value: &FuelPolicies) -> Self {
        Self {
            values: PolicyType::iter()
                .map(|t| value.get(t).unwrap_or_default())
                .collect(),
        }
    }
}

impl From<FuelUtxoId> for UtxoId {
    fn from(value: FuelUtxoId) -> Self {
        Self {
            tx_id: value.tx_id().to_vec(),
            output_index: value.output_index().into(),
        }
    }
}

impl From<FuelTxPointer> for TxPointer {
    fn from(value: FuelTxPointer) -> Self {
        Self {
            block_height: *value.block_height(),
            tx_index: value.tx_index().into(),
        }
    }
}

impl From<&FuelStorageSlot> for StorageSlot {
    fn from(value: &FuelStorageSlot) -> Self {
        Self {
            key: value.key().to_vec(),
            value: value.value().to_vec(),
        }
    }
}

impl From<&FuelReceipt> for Receipt {
    fn from(receipt: &FuelReceipt) -> Self {
        Self {
            kind: Some(match receipt {
                FuelReceipt::Call {
                    id,
                    to,
                    amount,
                    asset_id,
                    gas,
                    param1,
                    param2,
                    pc,
                    is,
                } => receipt::Kind::Call(CallReceipt {
                    id: (**id).into(),
                    to: (**to).into(),
                    amount: *amount,
                    asset_id: (**asset_id).into(),
                    gas: *gas,
                    param1: *param1,
                    param2: *param2,
                    pc: *pc,
                    is: *is,
                }),
                FuelReceipt::Return { id, val, pc, is } => {
                    receipt::Kind::Return(ReturnReceipt {
                        id: (**id).into(),
                        val: *val,
                        pc: *pc,
                        is: *is,
                    })
                }
                FuelReceipt::ReturnData {
                    id,
                    ptr,
                    len,
                    digest,
                    pc,
                    is,
                    data,
                } => receipt::Kind::ReturnData(ReturnDataReceipt {
                    id: (**id).into(),
                    ptr: *ptr,
                    len: *len,
                    digest: (**digest).into(),
                    pc: *pc,
                    is: *is,
                    data: data.clone().unwrap_or_default(),
                }),
                FuelReceipt::Panic {
                    id,
                    reason,
                    pc,
                    is,
                    contract_id,
                } => receipt::Kind::Panic(PanicReceipt {
                    id: (**id).into(),
                    reason: Some(reason.into()),
                    pc: *pc,
                    is: *is,
                    contract_id: contract_id.map(|v| (*v).into()).unwrap_or_default(),
                }),
                FuelReceipt::Revert { id, ra, pc, is } => {
                    receipt::Kind::Revert(RevertReceipt {
                        id: (**id).into(),
                        ra: *ra,
                        pc: *pc,
                        is: *is,
                    })
                }
                FuelReceipt::Log {
                    id,
                    ra,
                    rb,
                    rc,
                    rd,
                    pc,
                    is,
                } => receipt::Kind::Log(LogReceipt {
                    id: (**id).into(),
                    ra: *ra,
                    rb: *rb,
                    rc: *rc,
                    rd: *rd,
                    pc: *pc,
                    is: *is,
                }),
                FuelReceipt::LogData {
                    id,
                    ra,
                    rb,
                    ptr,
                    len,
                    digest,
                    pc,
                    is,
                    data,
                } => receipt::Kind::LogData(LogDataReceipt {
                    id: (**id).into(),
                    ra: *ra,
                    rb: *rb,
                    ptr: *ptr,
                    len: *len,
                    digest: (*digest).to_vec(),
                    pc: *pc,
                    is: *is,
                    data: data.clone().unwrap_or_default(),
                }),
                FuelReceipt::Transfer {
                    id,
                    to,
                    amount,
                    asset_id,
                    pc,
                    is,
                } => receipt::Kind::Transfer(TransferReceipt {
                    id: (**id).into(),
                    to: (**to).into(),
                    amount: *amount,
                    asset_id: (**asset_id).into(),
                    pc: *pc,
                    is: *is,
                }),
                FuelReceipt::TransferOut {
                    id,
                    to,
                    amount,
                    asset_id,
                    pc,
                    is,
                } => receipt::Kind::TransferOut(TransferOutReceipt {
                    id: (**id).into(),
                    to: (**to).into(),
                    amount: *amount,
                    asset_id: (**asset_id).into(),
                    pc: *pc,
                    is: *is,
                }),
                FuelReceipt::ScriptResult { result, gas_used } => {
                    receipt::Kind::ScriptResult(ScriptResultReceipt {
                        result: (*result).into(),
                        gas_used: *gas_used,
                    })
                }
                FuelReceipt::MessageOut {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    len,
                    digest,
                    data,
                } => receipt::Kind::MessageOut(MessageOutReceipt {
                    sender: (**sender).into(),
                    recipient: (**recipient).into(),
                    amount: *amount,
                    nonce: (**nonce).into(),
                    len: *len,
                    digest: (*digest).to_vec(),
                    data: data.clone().unwrap_or_default(),
                }),
                FuelReceipt::Mint {
                    sub_id,
                    contract_id,
                    val,
                    pc,
                    is,
                } => receipt::Kind::Mint(MintReceipt {
                    sub_id: (**sub_id).into(),
                    contract_id: (**contract_id).into(),
                    val: *val,
                    pc: *pc,
                    is: *is,
                }),
                FuelReceipt::Burn {
                    sub_id,
                    contract_id,
                    val,
                    pc,
                    is,
                } => receipt::Kind::Burn(BurnReceipt {
                    sub_id: (**sub_id).into(),
                    contract_id: (**contract_id).into(),
                    val: *val,
                    pc: *pc,
                    is: *is,
                }),
            }),
        }
    }
}

impl From<&FuelPanicInstruction> for PanicInstruction {
    fn from(instr: &FuelPanicInstruction) -> Self {
        Self {
            reason: (*instr.reason()) as u8 as u32,
            raw_instruction: *instr.instruction(),
        }
    }
}
