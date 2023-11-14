use super::{
    input::Input,
    output::Output,
    receipt::Receipt,
};
use crate::{
    fuel_core_graphql_api::{
        service::{
            Database,
            TxPool,
        },
        Config,
        IntoApiResult,
    },
    query::{
        SimpleBlockData,
        SimpleTransactionData,
        TransactionQueryData,
    },
    schema::{
        block::Block,
        contract::Contract,
        scalars::{
            AssetId,
            Bytes32,
            HexString,
            Salt,
            Tai64Timestamp,
            TransactionId,
            TxPointer,
            U32,
            U64,
        },
        tx::{
            input,
            output,
        },
    },
};
use async_graphql::{
    Context,
    Enum,
    Object,
    Union,
};
use fuel_core_storage::Error as StorageError;
use fuel_core_types::{
    blockchain::primitives,
    fuel_tx::{
        self,
        field::{
            BytecodeLength,
            BytecodeWitnessIndex,
            InputContract,
            Inputs,
            Maturity,
            MintAmount,
            MintAssetId,
            OutputContract,
            Outputs,
            Policies as PoliciesField,
            ReceiptsRoot,
            Salt as SaltField,
            Script as ScriptField,
            ScriptData,
            ScriptGasLimit,
            StorageSlots,
            TxPointer as TxPointerField,
            Witnesses,
        },
        policies::PolicyType,
        Chargeable,
        Executable,
    },
    fuel_types::canonical::Serialize,
    fuel_vm::ProgramState as VmProgramState,
    services::{
        txpool,
        txpool::TransactionStatus as TxStatus,
    },
    tai64::Tai64,
};

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
            VmProgramState::RunProgram(_) | VmProgramState::VerifyPredicate(_) => {
                unreachable!("This shouldn't get called with a debug state")
            }
        }
    }
}

#[derive(Union, Debug)]
pub enum TransactionStatus {
    Submitted(SubmittedStatus),
    Success(SuccessStatus),
    SqueezedOut(SqueezedOutStatus),
    Failed(FailureStatus),
}

#[derive(Debug)]
pub struct SubmittedStatus(pub Tai64);

#[Object]
impl SubmittedStatus {
    async fn time(&self) -> Tai64Timestamp {
        Tai64Timestamp(self.0)
    }
}

#[derive(Debug)]
pub struct SuccessStatus {
    block_id: primitives::BlockId,
    time: Tai64,
    result: Option<VmProgramState>,
}

#[Object]
impl SuccessStatus {
    async fn block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query: &Database = ctx.data_unchecked();
        let block = query.block(&self.block_id)?;
        Ok(block.into())
    }

    async fn time(&self) -> Tai64Timestamp {
        Tai64Timestamp(self.time)
    }

    async fn program_state(&self) -> Option<ProgramState> {
        self.result.map(Into::into)
    }
}

#[derive(Debug)]
pub struct FailureStatus {
    block_id: primitives::BlockId,
    time: Tai64,
    reason: String,
    state: Option<VmProgramState>,
}

#[Object]
impl FailureStatus {
    async fn block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query: &Database = ctx.data_unchecked();
        let block = query.block(&self.block_id)?;
        Ok(block.into())
    }

    async fn time(&self) -> Tai64Timestamp {
        Tai64Timestamp(self.time)
    }

    async fn reason(&self) -> String {
        self.reason.clone()
    }

    async fn program_state(&self) -> Option<ProgramState> {
        self.state.map(Into::into)
    }
}

#[derive(Debug)]
pub struct SqueezedOutStatus {
    pub reason: String,
}

#[Object]
impl SqueezedOutStatus {
    async fn reason(&self) -> String {
        self.reason.clone()
    }
}

impl From<TxStatus> for TransactionStatus {
    fn from(s: TxStatus) -> Self {
        match s {
            TxStatus::Submitted { time } => {
                TransactionStatus::Submitted(SubmittedStatus(time))
            }
            TxStatus::Success {
                block_id,
                result,
                time,
            } => TransactionStatus::Success(SuccessStatus {
                block_id,
                result,
                time,
            }),
            TxStatus::SqueezedOut { reason } => {
                TransactionStatus::SqueezedOut(SqueezedOutStatus { reason })
            }
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

impl From<TransactionStatus> for TxStatus {
    fn from(s: TransactionStatus) -> Self {
        match s {
            TransactionStatus::Submitted(SubmittedStatus(time)) => {
                TxStatus::Submitted { time }
            }
            TransactionStatus::Success(SuccessStatus {
                block_id,
                result,
                time,
            }) => TxStatus::Success {
                block_id,
                result,
                time,
            },
            TransactionStatus::SqueezedOut(SqueezedOutStatus { reason }) => {
                TxStatus::SqueezedOut { reason }
            }
            TransactionStatus::Failed(FailureStatus {
                block_id,
                reason,
                time,
                state: result,
            }) => TxStatus::Failed {
                block_id,
                reason,
                time,
                result,
            },
        }
    }
}

pub struct Policies(fuel_tx::policies::Policies);

#[Object]
impl Policies {
    async fn gas_price(&self) -> Option<U64> {
        self.0.get(PolicyType::GasPrice).map(Into::into)
    }

    async fn witness_limit(&self) -> Option<U64> {
        self.0.get(PolicyType::WitnessLimit).map(Into::into)
    }

    async fn maturity(&self) -> Option<U32> {
        self.0
            .get(PolicyType::Maturity)
            .and_then(|value| u32::try_from(value).ok())
            .map(Into::into)
    }

    async fn max_fee(&self) -> Option<U64> {
        self.0.get(PolicyType::MaxFee).map(Into::into)
    }
}

pub struct Transaction(pub(crate) fuel_tx::Transaction, pub(crate) fuel_tx::TxId);

impl Transaction {
    pub fn from_tx(id: fuel_tx::TxId, tx: fuel_tx::Transaction) -> Self {
        Self(tx, id)
    }
}

#[Object]
impl Transaction {
    async fn id(&self) -> TransactionId {
        TransactionId(self.1)
    }

    async fn input_asset_ids(&self, ctx: &Context<'_>) -> Option<Vec<AssetId>> {
        let config = ctx.data_unchecked::<Config>();
        let base_asset_id = config.consensus_parameters.base_asset_id();
        match &self.0 {
            fuel_tx::Transaction::Script(script) => Some(
                script
                    .input_asset_ids(base_asset_id)
                    .map(|c| AssetId(*c))
                    .collect(),
            ),
            fuel_tx::Transaction::Create(create) => Some(
                create
                    .input_asset_ids(base_asset_id)
                    .map(|c| AssetId(*c))
                    .collect(),
            ),
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn input_contracts(&self) -> Option<Vec<Contract>> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some(script.input_contracts().map(|v| Contract(*v)).collect())
            }
            fuel_tx::Transaction::Create(create) => {
                Some(create.input_contracts().map(|v| Contract(*v)).collect())
            }
            fuel_tx::Transaction::Mint(mint) => {
                Some(vec![Contract(mint.input_contract().contract_id)])
            }
        }
    }

    async fn input_contract(&self) -> Option<input::InputContract> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) | fuel_tx::Transaction::Create(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some(mint.input_contract().into()),
        }
    }

    async fn policies(&self) -> Option<Policies> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => Some((*script.policies()).into()),
            fuel_tx::Transaction::Create(create) => Some((*create.policies()).into()),
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn gas_price(&self) -> Option<U64> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => Some(script.price().into()),
            fuel_tx::Transaction::Create(create) => Some(create.price().into()),
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn script_gas_limit(&self) -> Option<U64> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some((*script.script_gas_limit()).into())
            }
            fuel_tx::Transaction::Create(_) => Some(0.into()),
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn maturity(&self) -> Option<U32> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => Some(script.maturity().into()),
            fuel_tx::Transaction::Create(create) => Some(create.maturity().into()),
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn mint_amount(&self) -> Option<U64> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) | fuel_tx::Transaction::Create(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some((*mint.mint_amount()).into()),
        }
    }

    async fn mint_asset_id(&self) -> Option<AssetId> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) | fuel_tx::Transaction::Create(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some((*mint.mint_asset_id()).into()),
        }
    }

    // TODO: Maybe we need to do the same `Script` and `Create`
    async fn tx_pointer(&self) -> Option<TxPointer> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) => None,
            fuel_tx::Transaction::Create(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some((*mint.tx_pointer()).into()),
        }
    }

    async fn is_script(&self) -> bool {
        self.0.is_script()
    }

    async fn is_create(&self) -> bool {
        self.0.is_create()
    }

    async fn is_mint(&self) -> bool {
        self.0.is_mint()
    }

    async fn inputs(&self) -> Option<Vec<Input>> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some(script.inputs().iter().map(Into::into).collect())
            }
            fuel_tx::Transaction::Create(create) => {
                Some(create.inputs().iter().map(Into::into).collect())
            }
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn outputs(&self) -> Vec<Output> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                script.outputs().iter().map(Into::into).collect()
            }
            fuel_tx::Transaction::Create(create) => {
                create.outputs().iter().map(Into::into).collect()
            }
            fuel_tx::Transaction::Mint(_) => vec![],
        }
    }

    async fn output_contract(&self) -> Option<output::ContractOutput> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) | fuel_tx::Transaction::Create(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some(mint.output_contract().into()),
        }
    }

    async fn witnesses(&self) -> Option<Vec<HexString>> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => Some(
                script
                    .witnesses()
                    .iter()
                    .map(|w| HexString(w.clone().into_inner()))
                    .collect(),
            ),
            fuel_tx::Transaction::Create(create) => Some(
                create
                    .witnesses()
                    .iter()
                    .map(|w| HexString(w.clone().into_inner()))
                    .collect(),
            ),
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn receipts_root(&self) -> Option<Bytes32> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some((*script.receipts_root()).into())
            }
            fuel_tx::Transaction::Create(_) => None,
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn status(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<TransactionStatus>> {
        let id = self.1;
        let query: &Database = ctx.data_unchecked();
        let txpool = ctx.data_unchecked::<TxPool>();
        get_tx_status(id, query, txpool).map_err(Into::into)
    }

    async fn receipts(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<Vec<Receipt>>> {
        let query: &Database = ctx.data_unchecked();
        let receipts = query
            .receipts(&self.1)
            .into_api_result::<Vec<_>, async_graphql::Error>()?;
        Ok(receipts.map(|receipts| receipts.into_iter().map(Receipt).collect()))
    }

    async fn script(&self) -> Option<HexString> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some(HexString(script.script().clone()))
            }
            fuel_tx::Transaction::Create(_) => None,
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn script_data(&self) -> Option<HexString> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some(HexString(script.script_data().clone()))
            }
            fuel_tx::Transaction::Create(_) => None,
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn bytecode_witness_index(&self) -> Option<u8> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) => None,
            fuel_tx::Transaction::Create(create) => {
                Some(*create.bytecode_witness_index())
            }
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn bytecode_length(&self) -> Option<U64> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) => None,
            fuel_tx::Transaction::Create(create) => {
                Some((*create.bytecode_length()).into())
            }
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn salt(&self) -> Option<Salt> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) => None,
            fuel_tx::Transaction::Create(create) => Some((*create.salt()).into()),
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    async fn storage_slots(&self) -> Option<Vec<HexString>> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) => None,
            fuel_tx::Transaction::Create(create) => Some(
                create
                    .storage_slots()
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
            fuel_tx::Transaction::Mint(_) => None,
        }
    }

    /// Return the transaction bytes using canonical encoding
    async fn raw_payload(&self) -> HexString {
        HexString(self.0.clone().to_bytes())
    }
}

#[tracing::instrument(level = "debug", skip(query, txpool), ret, err)]
pub(crate) fn get_tx_status(
    id: fuel_core_types::fuel_types::Bytes32,
    query: &Database,
    txpool: &TxPool,
) -> Result<Option<TransactionStatus>, StorageError> {
    match query
        .status(&id)
        .into_api_result::<txpool::TransactionStatus, StorageError>()?
    {
        Some(status) => Ok(Some(status.into())),
        None => match txpool.submission_time(id) {
            Some(submitted_time) => Ok(Some(TransactionStatus::Submitted(
                SubmittedStatus(submitted_time),
            ))),
            _ => Ok(None),
        },
    }
}

impl From<fuel_tx::policies::Policies> for Policies {
    fn from(value: fuel_tx::policies::Policies) -> Self {
        Policies(value)
    }
}
