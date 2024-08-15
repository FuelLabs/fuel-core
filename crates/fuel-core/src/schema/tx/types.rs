use super::{
    input::Input,
    output::Output,
    receipt::Receipt,
};
use crate::{
    fuel_core_graphql_api::{
        api_service::{
            ConsensusProvider,
            TxPool,
        },
        database::ReadView,
        IntoApiResult,
        QUERY_COSTS,
    },
    query::{
        SimpleBlockData,
        SimpleTransactionData,
        TransactionQueryData,
    },
    schema::{
        block::Block,
        scalars::{
            AssetId,
            BlobId,
            Bytes32,
            ContractId,
            HexString,
            Salt,
            Tai64Timestamp,
            TransactionId,
            TxPointer,
            U16,
            U32,
            U64,
        },
        tx::{
            input,
            output,
            upgrade_purpose::UpgradePurpose,
        },
        ReadViewProvider,
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
    fuel_tx::{
        self,
        field::{
            BytecodeRoot,
            BytecodeWitnessIndex,
            ChargeableBody,
            InputContract,
            Inputs,
            Maturity,
            MintAmount,
            MintAssetId,
            MintGasPrice,
            OutputContract,
            Outputs,
            Policies as PoliciesField,
            ProofSet,
            ReceiptsRoot,
            Salt as SaltField,
            Script as ScriptField,
            ScriptData,
            ScriptGasLimit,
            StorageSlots,
            SubsectionIndex,
            SubsectionsNumber,
            TxPointer as TxPointerField,
            UpgradePurpose as UpgradePurposeField,
            Witnesses,
        },
        policies::PolicyType,
        Executable,
        TxId,
    },
    fuel_types::canonical::Serialize,
    fuel_vm::ProgramState as VmProgramState,
    services::{
        executor::{
            TransactionExecutionResult,
            TransactionExecutionStatus,
        },
        txpool::{
            self,
            TransactionStatus as TxStatus,
        },
    },
    tai64::Tai64,
};
use std::vec::IntoIter;

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
    tx_id: TxId,
    block_height: fuel_core_types::fuel_types::BlockHeight,
    time: Tai64,
    result: Option<VmProgramState>,
    receipts: Vec<fuel_tx::Receipt>,
    total_gas: u64,
    total_fee: u64,
}

#[Object]
impl SuccessStatus {
    async fn transaction_id(&self) -> TransactionId {
        self.tx_id.into()
    }

    async fn block_height(&self) -> U32 {
        self.block_height.into()
    }

    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query = ctx.read_view()?;
        let block = query.block(&self.block_height)?;
        Ok(block.into())
    }

    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn transaction(&self, ctx: &Context<'_>) -> async_graphql::Result<Transaction> {
        let query = ctx.read_view()?;
        let transaction = query.transaction(&self.tx_id)?;
        Ok(Transaction::from_tx(self.tx_id, transaction))
    }

    async fn time(&self) -> Tai64Timestamp {
        Tai64Timestamp(self.time)
    }

    async fn program_state(&self) -> Option<ProgramState> {
        self.result.map(Into::into)
    }

    async fn receipts(&self) -> async_graphql::Result<Vec<Receipt>> {
        Ok(self.receipts.iter().map(Into::into).collect())
    }

    async fn total_gas(&self) -> U64 {
        self.total_gas.into()
    }

    async fn total_fee(&self) -> U64 {
        self.total_fee.into()
    }
}

#[derive(Debug)]
pub struct FailureStatus {
    tx_id: TxId,
    block_height: fuel_core_types::fuel_types::BlockHeight,
    time: Tai64,
    state: Option<VmProgramState>,
    receipts: Vec<fuel_tx::Receipt>,
    total_gas: u64,
    total_fee: u64,
}

#[Object]
impl FailureStatus {
    async fn transaction_id(&self) -> TransactionId {
        self.tx_id.into()
    }

    async fn block_height(&self) -> U32 {
        self.block_height.into()
    }

    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query = ctx.read_view()?;
        let block = query.block(&self.block_height)?;
        Ok(block.into())
    }

    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn transaction(&self, ctx: &Context<'_>) -> async_graphql::Result<Transaction> {
        let query = ctx.read_view()?;
        let transaction = query.transaction(&self.tx_id)?;
        Ok(Transaction::from_tx(self.tx_id, transaction))
    }

    async fn time(&self) -> Tai64Timestamp {
        Tai64Timestamp(self.time)
    }

    async fn reason(&self) -> String {
        TransactionExecutionResult::reason(&self.receipts, &self.state)
    }

    async fn program_state(&self) -> Option<ProgramState> {
        self.state.map(Into::into)
    }

    async fn receipts(&self) -> async_graphql::Result<Vec<Receipt>> {
        Ok(self.receipts.iter().map(Into::into).collect())
    }

    async fn total_gas(&self) -> U64 {
        self.total_gas.into()
    }

    async fn total_fee(&self) -> U64 {
        self.total_fee.into()
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

impl TransactionStatus {
    pub fn new(tx_id: TxId, tx_status: TxStatus) -> Self {
        match tx_status {
            TxStatus::Submitted { time } => {
                TransactionStatus::Submitted(SubmittedStatus(time))
            }
            TxStatus::Success {
                block_height,
                result,
                time,
                receipts,
                total_gas,
                total_fee,
            } => TransactionStatus::Success(SuccessStatus {
                tx_id,
                block_height,
                result,
                time,
                receipts,
                total_gas,
                total_fee,
            }),
            TxStatus::SqueezedOut { reason } => {
                TransactionStatus::SqueezedOut(SqueezedOutStatus { reason })
            }
            TxStatus::Failed {
                block_height,
                time,
                result,
                receipts,
                total_gas,
                total_fee,
            } => TransactionStatus::Failed(FailureStatus {
                tx_id,
                block_height,
                time,
                state: result,
                receipts,
                total_gas,
                total_fee,
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
                block_height,
                result,
                time,
                receipts,
                total_gas,
                total_fee,
                ..
            }) => TxStatus::Success {
                block_height,
                result,
                time,
                receipts,
                total_gas,
                total_fee,
            },
            TransactionStatus::SqueezedOut(SqueezedOutStatus { reason }) => {
                TxStatus::SqueezedOut { reason }
            }
            TransactionStatus::Failed(FailureStatus {
                block_height,
                time,
                state: result,
                receipts,
                total_gas,
                total_fee,
                ..
            }) => TxStatus::Failed {
                block_height,
                time,
                result,
                receipts,
                total_gas,
                total_fee,
            },
        }
    }
}

pub struct Policies(fuel_tx::policies::Policies);

#[Object]
impl Policies {
    async fn tip(&self) -> Option<U64> {
        self.0.get(PolicyType::Tip).map(Into::into)
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

    #[graphql(complexity = "QUERY_COSTS.storage_read")]
    async fn input_asset_ids(&self, ctx: &Context<'_>) -> Option<Vec<AssetId>> {
        let params = ctx
            .data_unchecked::<ConsensusProvider>()
            .latest_consensus_params();
        let base_asset_id = params.base_asset_id();
        match &self.0 {
            fuel_tx::Transaction::Script(tx) => Some(
                tx.input_asset_ids(base_asset_id)
                    .map(|c| AssetId(*c))
                    .collect(),
            ),
            fuel_tx::Transaction::Create(tx) => Some(
                tx.input_asset_ids(base_asset_id)
                    .map(|c| AssetId(*c))
                    .collect(),
            ),
            fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(tx) => Some(
                tx.input_asset_ids(base_asset_id)
                    .map(|c| AssetId(*c))
                    .collect(),
            ),
            fuel_tx::Transaction::Upload(tx) => Some(
                tx.input_asset_ids(base_asset_id)
                    .map(|c| AssetId(*c))
                    .collect(),
            ),
            fuel_tx::Transaction::Blob(tx) => Some(
                tx.input_asset_ids(base_asset_id)
                    .map(|c| AssetId(*c))
                    .collect(),
            ),
        }
    }

    async fn input_contracts(&self) -> Option<Vec<ContractId>> {
        match &self.0 {
            fuel_tx::Transaction::Script(tx) => {
                Some(input_contracts(tx).map(|v| (*v).into()).collect())
            }
            fuel_tx::Transaction::Create(tx) => {
                Some(input_contracts(tx).map(|v| (*v).into()).collect())
            }
            fuel_tx::Transaction::Mint(mint) => {
                Some(vec![mint.input_contract().contract_id.into()])
            }
            fuel_tx::Transaction::Upgrade(tx) => {
                Some(input_contracts(tx).map(|v| (*v).into()).collect())
            }
            fuel_tx::Transaction::Upload(tx) => {
                Some(input_contracts(tx).map(|v| (*v).into()).collect())
            }
            fuel_tx::Transaction::Blob(tx) => {
                Some(input_contracts(tx).map(|v| (*v).into()).collect())
            }
        }
    }

    async fn input_contract(&self) -> Option<input::InputContract> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some(mint.input_contract().into()),
        }
    }

    async fn policies(&self) -> Option<Policies> {
        match &self.0 {
            fuel_tx::Transaction::Script(tx) => Some((*tx.policies()).into()),
            fuel_tx::Transaction::Create(tx) => Some((*tx.policies()).into()),
            fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(tx) => Some((*tx.policies()).into()),
            fuel_tx::Transaction::Upload(tx) => Some((*tx.policies()).into()),
            fuel_tx::Transaction::Blob(tx) => Some((*tx.policies()).into()),
        }
    }

    async fn script_gas_limit(&self) -> Option<U64> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some((*script.script_gas_limit()).into())
            }
            fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
        }
    }

    async fn maturity(&self) -> Option<U32> {
        match &self.0 {
            fuel_tx::Transaction::Script(tx) => Some(tx.maturity().into()),
            fuel_tx::Transaction::Create(tx) => Some(tx.maturity().into()),
            fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(tx) => Some(tx.maturity().into()),
            fuel_tx::Transaction::Upload(tx) => Some(tx.maturity().into()),
            fuel_tx::Transaction::Blob(tx) => Some(tx.maturity().into()),
        }
    }

    async fn mint_amount(&self) -> Option<U64> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some((*mint.mint_amount()).into()),
        }
    }

    async fn mint_asset_id(&self) -> Option<AssetId> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some((*mint.mint_asset_id()).into()),
        }
    }

    async fn mint_gas_price(&self) -> Option<U64> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some((*mint.gas_price()).into()),
        }
    }

    // TODO: Maybe we need to do the same `Script` and `Create`
    async fn tx_pointer(&self) -> Option<TxPointer> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
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

    async fn is_upgrade(&self) -> bool {
        self.0.is_upgrade()
    }

    async fn is_upload(&self) -> bool {
        self.0.is_upload()
    }

    async fn is_blob(&self) -> bool {
        self.0.is_blob()
    }

    async fn inputs(&self) -> Option<Vec<Input>> {
        match &self.0 {
            fuel_tx::Transaction::Script(tx) => {
                Some(tx.inputs().iter().map(Into::into).collect())
            }
            fuel_tx::Transaction::Create(tx) => {
                Some(tx.inputs().iter().map(Into::into).collect())
            }
            fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(tx) => {
                Some(tx.inputs().iter().map(Into::into).collect())
            }
            fuel_tx::Transaction::Upload(tx) => {
                Some(tx.inputs().iter().map(Into::into).collect())
            }
            fuel_tx::Transaction::Blob(tx) => {
                Some(tx.inputs().iter().map(Into::into).collect())
            }
        }
    }

    async fn outputs(&self) -> Vec<Output> {
        match &self.0 {
            fuel_tx::Transaction::Script(tx) => {
                tx.outputs().iter().map(Into::into).collect()
            }
            fuel_tx::Transaction::Create(tx) => {
                tx.outputs().iter().map(Into::into).collect()
            }
            fuel_tx::Transaction::Mint(_) => vec![],
            fuel_tx::Transaction::Upgrade(tx) => {
                tx.outputs().iter().map(Into::into).collect()
            }
            fuel_tx::Transaction::Upload(tx) => {
                tx.outputs().iter().map(Into::into).collect()
            }
            fuel_tx::Transaction::Blob(tx) => {
                tx.outputs().iter().map(Into::into).collect()
            }
        }
    }

    async fn output_contract(&self) -> Option<output::ContractOutput> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Mint(mint) => Some(mint.output_contract().into()),
        }
    }

    async fn witnesses(&self) -> Option<Vec<HexString>> {
        match &self.0 {
            fuel_tx::Transaction::Script(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString(w.clone().into_inner()))
                    .collect(),
            ),
            fuel_tx::Transaction::Create(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString(w.clone().into_inner()))
                    .collect(),
            ),
            fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString(w.clone().into_inner()))
                    .collect(),
            ),
            fuel_tx::Transaction::Upload(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString(w.clone().into_inner()))
                    .collect(),
            ),
            fuel_tx::Transaction::Blob(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString(w.clone().into_inner()))
                    .collect(),
            ),
        }
    }

    async fn receipts_root(&self) -> Option<Bytes32> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some((*script.receipts_root()).into())
            }
            fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
        }
    }

    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn status(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<TransactionStatus>> {
        let id = self.1;
        let query = ctx.read_view()?;
        let txpool = ctx.data_unchecked::<TxPool>();
        get_tx_status(id, query.as_ref(), txpool).map_err(Into::into)
    }

    async fn script(&self) -> Option<HexString> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some(HexString(script.script().clone()))
            }
            fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
        }
    }

    async fn script_data(&self) -> Option<HexString> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some(HexString(script.script_data().clone()))
            }
            fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Upload(_)
            | fuel_tx::Transaction::Blob(_) => None,
        }
    }

    async fn bytecode_witness_index(&self) -> Option<U16> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) => None,
            fuel_tx::Transaction::Create(tx) => {
                Some((*tx.bytecode_witness_index()).into())
            }
            fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(_) => None,
            fuel_tx::Transaction::Upload(tx) => {
                Some((*tx.bytecode_witness_index()).into())
            }
            fuel_tx::Transaction::Blob(tx) => Some((*tx.bytecode_witness_index()).into()),
        }
    }

    async fn blob_id(&self) -> Option<BlobId> {
        match &self.0 {
            fuel_tx::Transaction::Blob(blob) => Some(blob.body().id.into()),
            _ => None,
        }
    }

    async fn salt(&self) -> Option<Salt> {
        match &self.0 {
            fuel_tx::Transaction::Script(_) => None,
            fuel_tx::Transaction::Create(create) => Some((*create.salt()).into()),
            fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(_) => None,
            fuel_tx::Transaction::Upload(_) => None,
            fuel_tx::Transaction::Blob(_) => None,
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
            fuel_tx::Transaction::Upgrade(_) => None,
            fuel_tx::Transaction::Upload(_) => None,
            fuel_tx::Transaction::Blob(_) => None,
        }
    }

    async fn bytecode_root(&self) -> Option<Bytes32> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Upload(tx) => Some((*tx.bytecode_root()).into()),
        }
    }

    async fn subsection_index(&self) -> Option<U16> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Upload(tx) => Some((*tx.subsection_index()).into()),
        }
    }

    async fn subsections_number(&self) -> Option<U16> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Upload(tx) => Some((*tx.subsections_number()).into()),
        }
    }

    async fn proof_set(&self) -> Option<Vec<Bytes32>> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_)
            | fuel_tx::Transaction::Upgrade(_)
            | fuel_tx::Transaction::Blob(_) => None,
            fuel_tx::Transaction::Upload(tx) => {
                Some(tx.proof_set().iter().map(|proof| (*proof).into()).collect())
            }
        }
    }

    async fn upgrade_purpose(&self) -> Option<UpgradePurpose> {
        match &self.0 {
            fuel_tx::Transaction::Script(_)
            | fuel_tx::Transaction::Create(_)
            | fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(tx) => Some((*tx.upgrade_purpose()).into()),
            fuel_tx::Transaction::Upload(_) => None,
            fuel_tx::Transaction::Blob(_) => None,
        }
    }

    #[graphql(complexity = "QUERY_COSTS.raw_payload")]
    /// Return the transaction bytes using canonical encoding
    async fn raw_payload(&self) -> HexString {
        HexString(self.0.clone().to_bytes())
    }
}

#[derive(Union, Debug)]
pub enum DryRunTransactionStatus {
    Success(DryRunSuccessStatus),
    Failed(DryRunFailureStatus),
}

impl DryRunTransactionStatus {
    pub fn new(tx_status: TransactionExecutionResult) -> Self {
        match tx_status {
            TransactionExecutionResult::Success {
                result,
                receipts,
                total_gas,
                total_fee,
            } => DryRunTransactionStatus::Success(DryRunSuccessStatus {
                result,
                receipts,
                total_gas,
                total_fee,
            }),
            TransactionExecutionResult::Failed {
                result,
                receipts,
                total_gas,
                total_fee,
            } => DryRunTransactionStatus::Failed(DryRunFailureStatus {
                result,
                receipts,
                total_gas,
                total_fee,
            }),
        }
    }
}

fn input_contracts<Tx>(tx: &Tx) -> IntoIter<&fuel_core_types::fuel_types::ContractId>
where
    Tx: Inputs,
{
    let mut inputs: Vec<_> = tx
        .inputs()
        .iter()
        .filter_map(|input| match input {
            fuel_tx::Input::Contract(fuel_tx::input::contract::Contract {
                contract_id,
                ..
            }) => Some(contract_id),
            _ => None,
        })
        .collect();
    inputs.sort();
    inputs.dedup();
    inputs.into_iter()
}

#[derive(Debug)]
pub struct DryRunSuccessStatus {
    result: Option<VmProgramState>,
    receipts: Vec<fuel_tx::Receipt>,
    total_gas: u64,
    total_fee: u64,
}

#[Object]
impl DryRunSuccessStatus {
    async fn program_state(&self) -> Option<ProgramState> {
        self.result.map(Into::into)
    }

    async fn receipts(&self) -> Vec<Receipt> {
        self.receipts.iter().map(Into::into).collect()
    }

    async fn total_gas(&self) -> U64 {
        self.total_gas.into()
    }

    async fn total_fee(&self) -> U64 {
        self.total_fee.into()
    }
}

#[derive(Debug)]
pub struct DryRunFailureStatus {
    result: Option<VmProgramState>,
    receipts: Vec<fuel_tx::Receipt>,
    total_gas: u64,
    total_fee: u64,
}

#[Object]
impl DryRunFailureStatus {
    async fn program_state(&self) -> Option<ProgramState> {
        self.result.map(Into::into)
    }

    async fn reason(&self) -> String {
        TransactionExecutionResult::reason(&self.receipts, &self.result)
    }

    async fn receipts(&self) -> Vec<Receipt> {
        self.receipts.iter().map(Into::into).collect()
    }

    async fn total_gas(&self) -> U64 {
        self.total_gas.into()
    }

    async fn total_fee(&self) -> U64 {
        self.total_fee.into()
    }
}

pub struct DryRunTransactionExecutionStatus(pub TransactionExecutionStatus);

#[Object]
impl DryRunTransactionExecutionStatus {
    async fn id(&self) -> TransactionId {
        TransactionId(self.0.id)
    }

    async fn status(&self) -> DryRunTransactionStatus {
        DryRunTransactionStatus::new(self.0.result.clone())
    }

    async fn receipts(&self) -> Vec<Receipt> {
        self.0.result.receipts().iter().map(Into::into).collect()
    }
}

#[tracing::instrument(level = "debug", skip(query, txpool), ret, err)]
pub(crate) fn get_tx_status(
    id: fuel_core_types::fuel_types::Bytes32,
    query: &ReadView,
    txpool: &TxPool,
) -> Result<Option<TransactionStatus>, StorageError> {
    match query
        .status(&id)
        .into_api_result::<txpool::TransactionStatus, StorageError>()?
    {
        Some(status) => {
            let status = TransactionStatus::new(id, status);
            Ok(Some(status))
        }
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
