use super::{
    ContextExt,
    input::Input,
    output::Output,
    receipt::Receipt,
};
use crate::{
    fuel_core_graphql_api::{
        IntoApiResult,
        api_service::ChainInfoProvider,
        database::ReadView,
        query_costs,
    },
    graphql_api::api_service::DynTxStatusManager,
    schema::{
        ReadViewProvider,
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
            output::ResolvedOutput,
            upgrade_purpose::UpgradePurpose,
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
    fuel_tx::{
        self,
        Executable,
        TxId,
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
    },
    fuel_types::canonical::Serialize,
    fuel_vm::ProgramState as VmProgramState,
    services::{
        executor::{
            TransactionExecutionResult,
            TransactionExecutionStatus,
        },
        transaction_status::{
            self,
            TransactionStatus as TxStatus,
        },
    },
    tai64::Tai64,
};
use std::{
    sync::Arc,
    vec::IntoIter,
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
    PreconfirmationSuccess(PreconfirmationSuccessStatus),
    SqueezedOut(SqueezedOutStatus),
    Failure(FailureStatus),
    PreconfirmationFailure(PreconfirmationFailureStatus),
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
    status: Arc<transaction_status::statuses::Success>,
}

#[Object]
impl SuccessStatus {
    async fn transaction_id(&self) -> TransactionId {
        self.tx_id.into()
    }

    async fn block_height(&self) -> U32 {
        self.status.block_height.into()
    }

    #[graphql(complexity = "query_costs().block_header + child_complexity")]
    async fn block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query = ctx.read_view()?;
        let block = query.block(&self.status.block_height)?;
        Ok(block.into())
    }

    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn transaction(&self, ctx: &Context<'_>) -> async_graphql::Result<Transaction> {
        let query = ctx.read_view()?;
        let transaction = query.transaction(&self.tx_id)?;
        Ok(Transaction::from_tx(self.tx_id, transaction))
    }

    async fn time(&self) -> Tai64Timestamp {
        Tai64Timestamp(self.status.block_timestamp)
    }

    async fn program_state(&self) -> Option<ProgramState> {
        self.status.program_state.map(Into::into)
    }

    async fn receipts(&self) -> async_graphql::Result<Vec<Receipt>> {
        Ok(self.status.receipts.iter().map(Into::into).collect())
    }

    async fn total_gas(&self) -> U64 {
        self.status.total_gas.into()
    }

    async fn total_fee(&self) -> U64 {
        self.status.total_fee.into()
    }
}

#[derive(Debug)]
pub struct PreconfirmationSuccessStatus {
    pub tx_id: TxId,
    pub status: Arc<transaction_status::statuses::PreConfirmationSuccess>,
}

#[Object]
impl PreconfirmationSuccessStatus {
    async fn tx_pointer(&self) -> TxPointer {
        self.status.tx_pointer.into()
    }

    async fn total_gas(&self) -> U64 {
        self.status.total_gas.into()
    }

    async fn total_fee(&self) -> U64 {
        self.status.total_fee.into()
    }

    async fn transaction_id(&self) -> TransactionId {
        self.tx_id.into()
    }

    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn transaction(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<Transaction>> {
        Ok(ctx
            .try_find_tx(self.tx_id)
            .await?
            .map(|tx| Transaction::from_tx(self.tx_id, tx)))
    }

    async fn receipts(&self) -> async_graphql::Result<Option<Vec<Receipt>>> {
        let receipts = self
            .status
            .receipts
            .as_ref()
            .map(|receipts| receipts.iter().map(Into::into).collect());
        Ok(receipts)
    }

    async fn resolved_outputs(
        &self,
    ) -> async_graphql::Result<Option<Vec<ResolvedOutput>>> {
        let outputs = self
            .status
            .resolved_outputs
            .as_ref()
            .map(|outputs| outputs.iter().map(|&x| x.into()).collect());
        Ok(outputs)
    }
}

#[derive(Debug)]
pub struct FailureStatus {
    tx_id: TxId,
    status: Arc<transaction_status::statuses::Failure>,
}

#[Object]
impl FailureStatus {
    async fn transaction_id(&self) -> TransactionId {
        self.tx_id.into()
    }

    async fn block_height(&self) -> U32 {
        self.status.block_height.into()
    }

    #[graphql(complexity = "query_costs().block_header + child_complexity")]
    async fn block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query = ctx.read_view()?;
        let block = query.block(&self.status.block_height)?;
        Ok(block.into())
    }

    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn transaction(&self, ctx: &Context<'_>) -> async_graphql::Result<Transaction> {
        let query = ctx.read_view()?;
        let transaction = query.transaction(&self.tx_id)?;
        Ok(Transaction::from_tx(self.tx_id, transaction))
    }

    async fn time(&self) -> Tai64Timestamp {
        Tai64Timestamp(self.status.block_timestamp)
    }

    async fn reason(&self) -> String {
        TransactionExecutionResult::reason(
            &self.status.receipts,
            &self.status.program_state,
        )
    }

    async fn program_state(&self) -> Option<ProgramState> {
        self.status.program_state.map(Into::into)
    }

    async fn receipts(&self) -> async_graphql::Result<Vec<Receipt>> {
        Ok(self.status.receipts.iter().map(Into::into).collect())
    }

    async fn total_gas(&self) -> U64 {
        self.status.total_gas.into()
    }

    async fn total_fee(&self) -> U64 {
        self.status.total_fee.into()
    }
}

#[derive(Debug)]
pub struct PreconfirmationFailureStatus {
    pub tx_id: TxId,
    pub status: Arc<transaction_status::statuses::PreConfirmationFailure>,
}

#[Object]
impl PreconfirmationFailureStatus {
    async fn reason(&self) -> String {
        self.status.reason.clone()
    }

    async fn tx_pointer(&self) -> TxPointer {
        self.status.tx_pointer.into()
    }

    async fn total_gas(&self) -> U64 {
        self.status.total_gas.into()
    }

    async fn total_fee(&self) -> U64 {
        self.status.total_fee.into()
    }

    async fn transaction_id(&self) -> TransactionId {
        self.tx_id.into()
    }

    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn transaction(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<Transaction>> {
        Ok(ctx
            .try_find_tx(self.tx_id)
            .await?
            .map(|tx| Transaction::from_tx(self.tx_id, tx)))
    }

    async fn receipts(&self) -> async_graphql::Result<Option<Vec<Receipt>>> {
        let receipts = self
            .status
            .receipts
            .as_ref()
            .map(|receipts| receipts.iter().map(Into::into).collect());
        Ok(receipts)
    }

    async fn resolved_outputs(
        &self,
    ) -> async_graphql::Result<Option<Vec<ResolvedOutput>>> {
        let outputs = self
            .status
            .resolved_outputs
            .as_ref()
            .map(|outputs| outputs.iter().map(|&x| x.into()).collect());
        Ok(outputs)
    }
}

#[derive(Debug)]
pub struct SqueezedOutStatus {
    pub tx_id: TxId,
    pub status: Arc<transaction_status::statuses::SqueezedOut>,
}

#[Object]
impl SqueezedOutStatus {
    async fn transaction_id(&self) -> TransactionId {
        self.tx_id.into()
    }

    async fn reason(&self) -> String {
        self.status.reason.clone()
    }
}

impl TransactionStatus {
    pub fn new(tx_id: TxId, tx_status: TxStatus) -> Self {
        match tx_status {
            TxStatus::Submitted(status) => {
                TransactionStatus::Submitted(SubmittedStatus(status.timestamp))
            }
            TxStatus::Success(status) => {
                TransactionStatus::Success(SuccessStatus { tx_id, status })
            }
            TxStatus::SqueezedOut(status) => {
                TransactionStatus::SqueezedOut(SqueezedOutStatus { status, tx_id })
            }
            TxStatus::Failure(status) => {
                TransactionStatus::Failure(FailureStatus { tx_id, status })
            }
            TxStatus::PreConfirmationSuccess(status) => {
                TransactionStatus::PreconfirmationSuccess(PreconfirmationSuccessStatus {
                    tx_id,
                    status,
                })
            }
            TxStatus::PreConfirmationSqueezedOut(status) => {
                TransactionStatus::SqueezedOut(SqueezedOutStatus {
                    tx_id,
                    status: Arc::new(status.as_ref().into()),
                })
            }
            TxStatus::PreConfirmationFailure(status) => {
                TransactionStatus::PreconfirmationFailure(PreconfirmationFailureStatus {
                    tx_id,
                    status,
                })
            }
        }
    }

    pub fn is_final(&self) -> bool {
        match self {
            TransactionStatus::Success(_)
            | TransactionStatus::Failure(_)
            | TransactionStatus::SqueezedOut(_) => true,
            TransactionStatus::Submitted(_)
            | TransactionStatus::PreconfirmationSuccess(_)
            | TransactionStatus::PreconfirmationFailure(_) => false,
        }
    }

    pub fn is_preconfirmation(&self) -> bool {
        matches!(
            self,
            TransactionStatus::PreconfirmationSuccess(_)
                | TransactionStatus::PreconfirmationFailure(_)
        )
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

    #[graphql(complexity = "query_costs().storage_read")]
    async fn input_asset_ids(&self, ctx: &Context<'_>) -> Option<Vec<AssetId>> {
        let params = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();
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

    async fn witnesses<'a>(&'a self) -> Option<Vec<HexString<'a>>> {
        match &self.0 {
            fuel_tx::Transaction::Script(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString::from(w.as_vec().as_ref()))
                    .collect(),
            ),
            fuel_tx::Transaction::Create(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString::from(w.as_vec().as_ref()))
                    .collect(),
            ),
            fuel_tx::Transaction::Mint(_) => None,
            fuel_tx::Transaction::Upgrade(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString::from(w.as_vec().as_ref()))
                    .collect(),
            ),
            fuel_tx::Transaction::Upload(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString::from(w.as_vec().as_ref()))
                    .collect(),
            ),
            fuel_tx::Transaction::Blob(tx) => Some(
                tx.witnesses()
                    .iter()
                    .map(|w| HexString::from(w.as_vec().as_ref()))
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

    #[graphql(complexity = "query_costs().tx_status_read + child_complexity")]
    async fn status(
        &self,
        ctx: &Context<'_>,
        include_preconfirmation: Option<bool>,
    ) -> async_graphql::Result<Option<TransactionStatus>> {
        let id = self.1;
        let query = ctx.read_view()?;

        let tx_status_manager = ctx.data_unchecked::<DynTxStatusManager>();

        get_tx_status(
            &id,
            query.as_ref(),
            tx_status_manager,
            include_preconfirmation.unwrap_or(false),
        )
        .await
        .map(|status| status.map(|status| TransactionStatus::new(id, status)))
        .map_err(Into::into)
    }

    async fn script(&self) -> Option<HexString> {
        match &self.0 {
            fuel_tx::Transaction::Script(script) => {
                Some(HexString::from(script.script().as_ref()))
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
                Some(HexString::from(script.script_data().as_ref()))
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

    #[graphql(complexity = "query_costs().tx_raw_payload")]
    /// Return the transaction bytes using canonical encoding
    async fn raw_payload(&self) -> HexString {
        HexString::from(self.0.to_bytes())
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

#[derive(Debug, Clone)]
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

pub struct DryRunStorageReads {
    pub tx_statuses: Vec<DryRunTransactionExecutionStatus>,
    pub storage_reads: Vec<StorageReadReplayEvent>,
}

#[Object]
impl DryRunStorageReads {
    async fn tx_statuses(&self) -> &[DryRunTransactionExecutionStatus] {
        &self.tx_statuses
    }

    async fn storage_reads(&self) -> &[StorageReadReplayEvent] {
        &self.storage_reads
    }
}

#[derive(Clone)]
pub struct StorageReadReplayEvent {
    column: U32,
    key: HexString<'static>,
    value: Option<HexString<'static>>,
}

impl From<fuel_core_types::services::executor::StorageReadReplayEvent>
    for StorageReadReplayEvent
{
    fn from(event: fuel_core_types::services::executor::StorageReadReplayEvent) -> Self {
        Self {
            column: event.column.into(),
            key: HexString(event.key.into()),
            value: event.value.map(|bytes|HexString(bytes.into())),
        }
    }
}

#[Object]
impl StorageReadReplayEvent {
    async fn column(&self) -> U32 {
        self.column
    }

    async fn key(&self) -> HexString {
        self.key.clone()
    }

    async fn value(&self) -> Option<HexString> {
        self.value.clone()
    }
}

#[tracing::instrument(level = "debug", skip(query, tx_status_manager), ret, err)]
pub(crate) async fn get_tx_status(
    id: &fuel_core_types::fuel_types::Bytes32,
    query: &ReadView,
    tx_status_manager: &DynTxStatusManager,
    include_preconfirmation: bool,
) -> Result<Option<transaction_status::TransactionStatus>, StorageError> {
    let api_result = query
        .tx_status(id)
        .into_api_result::<transaction_status::TransactionStatus, StorageError>()?;
    match api_result {
        Some(status) => Ok(Some(status)),
        None => {
            let status = tx_status_manager.status(*id).await?;
            match status {
                Some(status) => {
                    // Filter out preconfirmation statuses if not allowed. Converting to submitted status
                    // because it's the closest to the preconfirmation status.
                    // Having `now()` as timestamp isn't ideal but shouldn't cause much inconsistency.
                    if !include_preconfirmation
                        && status.is_preconfirmation()
                        && !status.is_final()
                    {
                        Ok(Some(transaction_status::TransactionStatus::submitted(
                            Tai64::now(),
                        )))
                    } else {
                        Ok(Some(status))
                    }
                }
                None => Ok(None),
            }
        }
    }
}

impl From<fuel_tx::policies::Policies> for Policies {
    fn from(value: fuel_tx::policies::Policies) -> Self {
        Policies(value)
    }
}

pub struct AssembleTransactionResult {
    pub tx_id: fuel_tx::TxId,
    pub tx: fuel_tx::Transaction,
    pub status: TransactionExecutionResult,
    pub gas_price: u64,
}

#[Object]
impl AssembleTransactionResult {
    async fn transaction(&self) -> Transaction {
        Transaction::from_tx(self.tx_id, self.tx.clone())
    }

    async fn status(&self) -> DryRunTransactionStatus {
        DryRunTransactionStatus::new(self.status.clone())
    }

    async fn gas_price(&self) -> U64 {
        self.gas_price.into()
    }
}
