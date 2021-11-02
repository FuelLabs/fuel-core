use crate::database::Database;
use crate::schema::scalars::{HexString, HexString256};
use crate::tx_pool::TransactionStatus as TxStatus;
use async_graphql::{Context, Object, Union};
use chrono::{DateTime, Utc};
use fuel_asm::Word;
use fuel_storage::Storage;
use fuel_tx::{Address, Bytes32, Color, ContractId, Receipt, Transaction as FuelTx};
use fuel_types::bytes::SerializableVec;
use fuel_vm::prelude::ProgramState;
use std::ops::Deref;

#[derive(Union)]
pub enum Input {
    Coin(InputCoin),
    Contract(InputContract),
}

pub struct InputCoin {
    utxo_id: HexString256,
    owner: HexString256,
    amount: Word,
    color: HexString256,
    witness_index: u8,
    maturity: Word,
    predicate: HexString,
    predicate_data: HexString,
}

#[Object]
impl InputCoin {
    async fn utxo_id(&self) -> HexString256 {
        self.utxo_id
    }

    async fn owner(&self) -> HexString256 {
        self.owner
    }

    async fn amount(&self) -> Word {
        self.amount
    }

    async fn color(&self) -> HexString256 {
        self.color
    }

    async fn witness_index(&self) -> u8 {
        self.witness_index
    }

    async fn maturity(&self) -> Word {
        self.maturity
    }

    async fn predicate(&self) -> HexString {
        self.predicate.clone()
    }

    async fn predicate_data(&self) -> HexString {
        self.predicate_data.clone()
    }
}

pub struct InputContract {
    utxo_id: HexString256,
    balance_root: HexString256,
    state_root: HexString256,
    contract_id: HexString256,
}

#[Object]
impl InputContract {
    async fn utxo_id(&self) -> HexString256 {
        self.utxo_id
    }

    async fn balance_root(&self) -> HexString256 {
        self.balance_root
    }

    async fn state_root(&self) -> HexString256 {
        self.state_root
    }

    async fn contract_id(&self) -> HexString256 {
        self.contract_id
    }
}

impl From<&fuel_tx::Input> for Input {
    fn from(input: &fuel_tx::Input) -> Self {
        match input {
            fuel_tx::Input::Coin {
                utxo_id,
                owner,
                amount,
                color,
                witness_index,
                maturity,
                predicate,
                predicate_data,
            } => Input::Coin(InputCoin {
                utxo_id: HexString256(*utxo_id.deref()),
                owner: HexString256(*owner.deref()),
                amount: *amount,
                color: HexString256(*color.deref()),
                witness_index: *witness_index,
                maturity: *maturity,
                predicate: HexString(predicate.clone()),
                predicate_data: HexString(predicate_data.clone()),
            }),
            fuel_tx::Input::Contract {
                utxo_id,
                balance_root,
                state_root,
                contract_id,
            } => Input::Contract(InputContract {
                utxo_id: HexString256(*utxo_id.deref()),
                balance_root: HexString256(*balance_root.deref()),
                state_root: HexString256(*state_root.deref()),
                contract_id: HexString256(*contract_id.deref()),
            }),
        }
    }
}

#[derive(Union)]
pub enum Output {
    Coin(CoinOutput),
    Contract(ContractOutput),
    Withdrawal(WithdrawalOutput),
    Change(ChangeOutput),
    Variable(VariableOutput),
    ContractCreated(ContractCreated),
}

pub struct CoinOutput {
    to: Address,
    amount: Word,
    color: Color,
}

#[Object]
impl CoinOutput {
    async fn to(&self) -> HexString256 {
        HexString256(*self.to.deref())
    }

    async fn amount(&self) -> Word {
        self.amount
    }

    async fn color(&self) -> HexString256 {
        HexString256(*self.color.deref())
    }
}

pub struct WithdrawalOutput(CoinOutput);

#[Object]
impl WithdrawalOutput {
    async fn to(&self) -> HexString256 {
        HexString256(*self.0.to.deref())
    }

    async fn amount(&self) -> Word {
        self.0.amount
    }

    async fn color(&self) -> HexString256 {
        HexString256(*self.0.color.deref())
    }
}

pub struct ChangeOutput(CoinOutput);

#[Object]
impl ChangeOutput {
    async fn to(&self) -> HexString256 {
        HexString256(*self.0.to.deref())
    }

    async fn amount(&self) -> Word {
        self.0.amount
    }

    async fn color(&self) -> HexString256 {
        HexString256(*self.0.color.deref())
    }
}

pub struct VariableOutput(CoinOutput);

#[Object]
impl VariableOutput {
    async fn to(&self) -> HexString256 {
        HexString256(*self.0.to.deref())
    }

    async fn amount(&self) -> Word {
        self.0.amount
    }

    async fn color(&self) -> HexString256 {
        HexString256(*self.0.color.deref())
    }
}

pub struct ContractOutput {
    input_index: u8,
    balance_root: Bytes32,
    state_root: Bytes32,
}

#[Object]
impl ContractOutput {
    async fn input_index(&self) -> u8 {
        self.input_index
    }

    async fn balance_root(&self) -> HexString256 {
        HexString256(*self.balance_root.deref())
    }

    async fn state_root(&self) -> HexString256 {
        HexString256(*self.state_root.deref())
    }
}

pub struct ContractCreated {
    contract_id: ContractId,
}

#[Object]
impl ContractCreated {
    async fn contract_id(&self) -> HexString256 {
        HexString256(*self.contract_id.deref())
    }
}

impl From<&fuel_tx::Output> for Output {
    fn from(output: &fuel_tx::Output) -> Self {
        match output {
            fuel_tx::Output::Coin { to, amount, color } => Output::Coin(CoinOutput {
                to: *to,
                amount: *amount,
                color: *color,
            }),
            fuel_tx::Output::Contract {
                input_index,
                balance_root,
                state_root,
            } => Output::Contract(ContractOutput {
                input_index: *input_index,
                balance_root: *balance_root,
                state_root: *state_root,
            }),
            fuel_tx::Output::Withdrawal { to, amount, color } => {
                Output::Withdrawal(WithdrawalOutput(CoinOutput {
                    to: *to,
                    amount: *amount,
                    color: *color,
                }))
            }
            fuel_tx::Output::Change { to, amount, color } => {
                Output::Change(ChangeOutput(CoinOutput {
                    to: *to,
                    amount: *amount,
                    color: *color,
                }))
            }
            fuel_tx::Output::Variable { to, amount, color } => {
                Output::Variable(VariableOutput(CoinOutput {
                    to: *to,
                    amount: *amount,
                    color: *color,
                }))
            }
            fuel_tx::Output::ContractCreated { contract_id } => {
                Output::ContractCreated(ContractCreated {
                    contract_id: *contract_id,
                })
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
    block_id: Bytes32,
    time: DateTime<Utc>,
    result: ProgramState,
}

#[Object]
impl SuccessStatus {
    async fn block_id(&self) -> HexString256 {
        HexString256(*self.block_id.deref())
    }

    async fn time(&self) -> DateTime<Utc> {
        self.time
    }

    async fn program_state(&self) -> HexString {
        match self.result {
            ProgramState::Return(word) => HexString(word.to_be_bytes().to_vec()),
            ProgramState::ReturnData(data) => HexString(data.deref().to_vec()),
            ProgramState::Revert(word) => HexString(word.to_be_bytes().to_vec()),
        }
    }
}

pub struct FailureStatus {
    block_id: Bytes32,
    time: DateTime<Utc>,
    reason: String,
}

#[Object]
impl FailureStatus {
    async fn block_id(&self) -> HexString256 {
        HexString256(*self.block_id.deref())
    }

    async fn time(&self) -> DateTime<Utc> {
        self.time
    }

    async fn reason(&self) -> String {
        self.reason.clone()
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
            } => TransactionStatus::Failed(FailureStatus {
                block_id,
                reason,
                time,
            }),
        }
    }
}

pub struct Transaction(pub(crate) FuelTx);

#[Object]
impl Transaction {
    async fn id(&self) -> HexString256 {
        HexString256(*self.0.id().deref())
    }

    async fn input_colors(&self) -> Vec<HexString256> {
        self.0
            .input_colors()
            .map(|c| HexString256(*c.deref()))
            .collect()
    }

    async fn input_contracts(&self) -> Vec<HexString256> {
        self.0
            .input_contracts()
            .map(|v| HexString256(*v.deref()))
            .collect()
    }

    async fn gas_price(&self) -> Word {
        self.0.gas_price()
    }

    async fn gas_limit(&self) -> Word {
        self.0.gas_limit()
    }

    async fn maturity(&self) -> Word {
        self.0.maturity()
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

    async fn receipts_root(&self) -> Option<HexString256> {
        self.0
            .receipts_root()
            .cloned()
            .map(|b| HexString256(*b.deref()))
    }

    async fn status(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<TransactionStatus>> {
        let db = ctx.data_unchecked::<Database>();
        let status = db.get_tx_status(&self.0.id())?;
        Ok(status.map(Into::into))
    }

    async fn receipts(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<Vec<super::receipt::Receipt>>> {
        let db = ctx.data_unchecked::<Database>();
        let receipts = Storage::<Bytes32, Vec<Receipt>>::get(db, &self.0.id())?;
        Ok(receipts.map(|receipts| {
            receipts
                .into_owned()
                .into_iter()
                .map(super::receipt::Receipt)
                .collect()
        }))
    }

    async fn script(&self) -> Option<HexString> {
        match &self.0 {
            FuelTx::Script { script, .. } => Some(HexString(script.clone())),
            FuelTx::Create { .. } => None,
        }
    }

    async fn script_data(&self) -> Option<HexString> {
        match &self.0 {
            FuelTx::Script { script_data, .. } => Some(HexString(script_data.clone())),
            FuelTx::Create { .. } => None,
        }
    }

    async fn bytecode_witness_index(&self) -> Option<u8> {
        match self.0 {
            FuelTx::Script { .. } => None,
            FuelTx::Create {
                bytecode_witness_index,
                ..
            } => Some(bytecode_witness_index),
        }
    }

    async fn salt(&self) -> Option<HexString256> {
        match self.0 {
            FuelTx::Script { .. } => None,
            FuelTx::Create { salt, .. } => Some(salt.into()),
        }
    }

    async fn static_contracts(&self) -> Option<Vec<HexString256>> {
        match &self.0 {
            FuelTx::Script { .. } => None,
            FuelTx::Create {
                static_contracts, ..
            } => Some(static_contracts.iter().cloned().map(Into::into).collect()),
        }
    }

    /// Return the transaction bytes using canonical encoding
    async fn raw_payload(&self) -> HexString {
        HexString(self.0.clone().to_bytes())
    }
}
