use super::{input::Input, output::Output, receipt::Receipt};
use crate::{
    database::Database,
    model::FuelBlockDb,
    schema::{
        block::Block,
        contract::Contract,
        scalars::{AssetId, Bytes32, HexString, Salt, TransactionId, U64},
    },
    tx_pool::TransactionStatus as TxStatus,
};
use async_graphql::{Context, Enum, Object, Union};
use chrono::{DateTime, Utc};
use fuel_core_interfaces::{
    common::{
        fuel_storage::Storage, fuel_tx, fuel_types, fuel_types::bytes::SerializableVec,
        fuel_vm::prelude::ProgramState as VmProgramState,
    },
    db::KvStoreError,
    txpool::TxPoolMpsc,
};
use fuel_txpool::Service as TxPoolService;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct ProgramState {
    return_type: ReturnType,
    data: Vec<u8>,
}

#[Object]
impl ProgramState {
    async fn return_type(&self) -> ReturnType {
        self.return_type
    }

    async fn data(&self) -> HexString {
        self.data.clone().into()
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ReturnType {
    Return,
    ReturnData,
    Revert,
}

impl From<VmProgramState> for ProgramState {
    fn from(state: VmProgramState) -> Self {
        match state {
            VmProgramState::Return(d) => ProgramState {
                return_type: ReturnType::Return,
                data: d.to_be_bytes().to_vec(),
            },
            VmProgramState::ReturnData(d) => ProgramState {
                return_type: ReturnType::ReturnData,
                data: d.as_ref().to_vec(),
            },
            VmProgramState::Revert(d) => ProgramState {
                return_type: ReturnType::Revert,
                data: d.to_be_bytes().to_vec(),
            },
            #[cfg(feature = "debug")]
            VmProgramState::RunProgram(_) | VmProgramState::VerifyPredicate(_) => {
                unreachable!("This shouldn't get called with a debug state")
            }
        }
    }
}

#[derive(Union)]
pub enum TransactionStatus {
    Submitted(SubmittedStatus),
    Success(SuccessStatus),
    Failed(FailureStatus),
}

pub struct SubmittedStatus(DateTime<Utc>);

#[Object]
impl SubmittedStatus {
    async fn time(&self) -> DateTime<Utc> {
        self.0
    }
}

pub struct SuccessStatus {
    block_id: fuel_types::Bytes32,
    time: DateTime<Utc>,
    result: VmProgramState,
}

#[Object]
impl SuccessStatus {
    async fn block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let db = ctx.data_unchecked::<Database>();
        let block = Storage::<fuel_types::Bytes32, FuelBlockDb>::get(db, &self.block_id)?
            .ok_or(KvStoreError::NotFound)?
            .into_owned();
        let block = Block(block);
        Ok(block)
    }

    async fn time(&self) -> DateTime<Utc> {
        self.time
    }

    async fn program_state(&self) -> ProgramState {
        self.result.into()
    }
}

pub struct FailureStatus {
    block_id: fuel_types::Bytes32,
    time: DateTime<Utc>,
    reason: String,
    state: Option<VmProgramState>,
}

#[Object]
impl FailureStatus {
    async fn block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let db = ctx.data_unchecked::<Database>();
        let block = Storage::<fuel_types::Bytes32, FuelBlockDb>::get(db, &self.block_id)?
            .ok_or(KvStoreError::NotFound)?
            .into_owned();
        let block = Block(block);
        Ok(block)
    }

    async fn time(&self) -> DateTime<Utc> {
        self.time
    }

    async fn reason(&self) -> String {
        self.reason.clone()
    }

    async fn program_state(&self) -> Option<ProgramState> {
        self.state.map(Into::into)
    }
}

impl From<TxStatus> for TransactionStatus {
    fn from(s: TxStatus) -> Self {
        match s {
            TxStatus::Submitted { time } => TransactionStatus::Submitted(SubmittedStatus(time)),
            TxStatus::Success {
                block_id,
                result,
                time,
            } => TransactionStatus::Success(SuccessStatus {
                block_id,
                result,
                time,
            }),
            TxStatus::Failed {
                block_id,
                reason,
                time,
                result,
            } => TransactionStatus::Failed(FailureStatus {
                block_id,
                reason,
                time,
                state: result,
            }),
        }
    }
}

pub struct Transaction(pub(crate) fuel_tx::Transaction);

#[Object]
impl Transaction {
    async fn id(&self) -> TransactionId {
        TransactionId(self.0.id())
    }

    async fn input_asset_ids(&self) -> Vec<AssetId> {
        self.0.input_asset_ids().map(|c| AssetId(*c)).collect()
    }

    async fn input_contracts(&self) -> Vec<Contract> {
        self.0.input_contracts().map(|v| Contract(*v)).collect()
    }

    async fn gas_price(&self) -> U64 {
        self.0.gas_price().into()
    }

    async fn gas_limit(&self) -> U64 {
        self.0.gas_limit().into()
    }

    async fn byte_price(&self) -> U64 {
        self.0.byte_price().into()
    }

    async fn maturity(&self) -> U64 {
        self.0.maturity().into()
    }

    async fn is_script(&self) -> bool {
        self.0.is_script()
    }

    async fn inputs(&self) -> Vec<Input> {
        self.0.inputs().iter().map(Into::into).collect()
    }

    async fn outputs(&self) -> Vec<Output> {
        self.0.outputs().iter().map(Into::into).collect()
    }

    async fn witnesses(&self) -> Vec<HexString> {
        self.0
            .witnesses()
            .iter()
            .map(|w| HexString(w.clone().into_inner()))
            .collect()
    }

    async fn receipts_root(&self) -> Option<Bytes32> {
        self.0.receipts_root().cloned().map(Bytes32)
    }

    async fn status(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TransactionStatus>> {
        let db = ctx.data_unchecked::<Database>();
        let txpool = ctx.data_unchecked::<Arc<TxPoolService>>();
        let id = self.0.id();

        let (response, receiver) = oneshot::channel();
        let _ = txpool
            .sender()
            .send(TxPoolMpsc::FindOne { id, response })
            .await;

        if let Ok(Some(transaction_in_pool)) = receiver.await {
            let time = transaction_in_pool.submited_time();
            Ok(Some(TransactionStatus::Submitted(SubmittedStatus(time))))
        } else {
            let status = db.get_tx_status(&self.0.id())?;
            Ok(status.map(Into::into))
        }
    }

    async fn receipts(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Vec<Receipt>>> {
        let db = ctx.data_unchecked::<Database>();
        let receipts =
            Storage::<fuel_types::Bytes32, Vec<fuel_tx::Receipt>>::get(db, &self.0.id())?;
        Ok(receipts.map(|receipts| receipts.iter().cloned().map(Receipt).collect()))
    }

    async fn script(&self) -> Option<HexString> {
        match &self.0 {
            fuel_tx::Transaction::Script { script, .. } => Some(HexString(script.clone())),
            fuel_tx::Transaction::Create { .. } => None,
        }
    }

    async fn script_data(&self) -> Option<HexString> {
        match &self.0 {
            fuel_tx::Transaction::Script { script_data, .. } => {
                Some(HexString(script_data.clone()))
            }
            fuel_tx::Transaction::Create { .. } => None,
        }
    }

    async fn bytecode_witness_index(&self) -> Option<u8> {
        match self.0 {
            fuel_tx::Transaction::Script { .. } => None,
            fuel_tx::Transaction::Create {
                bytecode_witness_index,
                ..
            } => Some(bytecode_witness_index),
        }
    }

    async fn bytecode_length(&self) -> Option<U64> {
        match self.0 {
            fuel_tx::Transaction::Script { .. } => None,
            fuel_tx::Transaction::Create {
                bytecode_length, ..
            } => Some(bytecode_length.into()),
        }
    }

    async fn salt(&self) -> Option<Salt> {
        match self.0 {
            fuel_tx::Transaction::Script { .. } => None,
            fuel_tx::Transaction::Create { salt, .. } => Some(salt.into()),
        }
    }

    async fn static_contracts(&self) -> Option<Vec<Contract>> {
        match &self.0 {
            fuel_tx::Transaction::Script { .. } => None,
            fuel_tx::Transaction::Create {
                static_contracts, ..
            } => Some(static_contracts.iter().cloned().map(Into::into).collect()),
        }
    }

    async fn storage_slots(&self) -> Option<Vec<HexString>> {
        match &self.0 {
            fuel_tx::Transaction::Script { .. } => None,
            fuel_tx::Transaction::Create { storage_slots, .. } => Some(
                storage_slots
                    .iter()
                    .map(|slot| {
                        HexString(
                            slot.key()
                                .as_slice()
                                .iter()
                                .chain(slot.value().as_slice())
                                .copied()
                                .collect(),
                        )
                    })
                    .collect(),
            ),
        }
    }

    /// Return the transaction bytes using canonical encoding
    async fn raw_payload(&self) -> HexString {
        HexString(self.0.clone().to_bytes())
    }
}
