use crate::client::schema::{
    schema,
    tx::{tests::transparent_receipt::Receipt, TransactionStatus, TxIdArgs},
    ConnectionArgs, ConversionError, HexString, HexString256, HexStringUtxoId, PageInfo,
};
use core::convert::{TryFrom, TryInto};
use fuel_tx::StorageSlot;
use fuel_types::Bytes32;
use itertools::Itertools;

/// Retrieves the transaction in opaque form
#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "TxIdArgs"
)]
pub struct TransactionQuery {
    #[arguments(id = &args.id)]
    pub transaction: Option<Transaction>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "ConnectionArgs"
)]
pub struct TransactionsQuery {
    #[arguments(after = &args.after, before = &args.before, first = &args.first, last = &args.last)]
    pub transactions: TransactionConnection,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TransactionConnection {
    pub edges: Option<Vec<Option<TransactionEdge>>>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TransactionEdge {
    pub cursor: String,
    pub node: Transaction,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Transaction {
    pub gas_limit: i32,
    pub gas_price: i32,
    pub byte_price: i32,
    pub id: HexString256,
    pub input_colors: Vec<HexString256>,
    pub input_contracts: Vec<HexString256>,
    pub inputs: Vec<Input>,
    pub is_script: bool,
    pub outputs: Vec<Output>,
    pub maturity: i32,
    pub receipts_root: Option<HexString256>,
    pub status: Option<TransactionStatus>,
    pub witnesses: Vec<HexString>,
    pub receipts: Option<Vec<Receipt>>,
    pub script: Option<HexString>,
    pub script_data: Option<HexString>,
    pub salt: Option<HexString256>,
    pub static_contracts: Option<Vec<HexString256>>,
    pub storage_slots: Option<Vec<HexString>>,
    pub bytecode_witness_index: Option<i32>,
}

impl TryFrom<Transaction> for fuel_vm::prelude::Transaction {
    type Error = ConversionError;

    fn try_from(tx: Transaction) -> Result<Self, Self::Error> {
        Ok(match tx.is_script {
            true => Self::Script {
                gas_price: tx.gas_price.try_into()?,
                gas_limit: tx.gas_limit.try_into()?,
                byte_price: tx.byte_price.try_into()?,
                maturity: tx.maturity.try_into()?,
                receipts_root: tx
                    .receipts_root
                    .ok_or_else(|| ConversionError::MissingField("receipts_root".to_string()))?
                    .into(),
                script: tx
                    .script
                    .ok_or_else(|| ConversionError::MissingField("script".to_string()))?
                    .into(),
                script_data: tx
                    .script_data
                    .ok_or_else(|| ConversionError::MissingField("script_data".to_string()))?
                    .into(),
                inputs: tx
                    .inputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Input>, ConversionError>>()?,
                outputs: tx
                    .outputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Output>, ConversionError>>()?,
                witnesses: tx.witnesses.into_iter().map(|w| w.0 .0.into()).collect(),
                metadata: None,
            },
            false => Self::Create {
                gas_price: tx.gas_price.try_into()?,
                gas_limit: tx.gas_limit.try_into()?,
                byte_price: tx.byte_price.try_into()?,
                maturity: tx.maturity.try_into()?,
                bytecode_witness_index: tx
                    .bytecode_witness_index
                    .ok_or_else(|| {
                        ConversionError::MissingField("bytecode_witness_index".to_string())
                    })?
                    .try_into()?,
                salt: tx
                    .salt
                    .ok_or_else(|| ConversionError::MissingField("salt".to_string()))?
                    .into(),
                static_contracts: tx
                    .static_contracts
                    .ok_or_else(|| ConversionError::MissingField("static_contracts".to_string()))?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                storage_slots: tx
                    .storage_slots
                    .ok_or_else(|| ConversionError::MissingField("storage_slots".to_string()))?
                    .into_iter()
                    .map(|slot| {
                        if slot.0 .0.len() != 64 {
                            return Err(ConversionError::BytesLength);
                        }
                        let key = &slot.0 .0[0..32];
                        let value = &slot.0 .0[32..];
                        Ok(StorageSlot::new(
                            // unwrap is safe because length is checked
                            Bytes32::try_from(key).map_err(|_| ConversionError::BytesLength)?,
                            Bytes32::try_from(value).map_err(|_| ConversionError::BytesLength)?,
                        ))
                    })
                    .try_collect()?,
                inputs: tx
                    .inputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Input>, ConversionError>>()?,
                outputs: tx
                    .outputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Output>, ConversionError>>()?,
                witnesses: tx.witnesses.into_iter().map(|w| w.0 .0.into()).collect(),
                metadata: None,
            },
        })
    }
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Input {
    InputCoin(InputCoin),
    InputContract(InputContract),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputCoin {
    pub utxo_id: HexStringUtxoId,
    pub owner: HexString256,
    pub amount: i32,
    pub color: HexString256,
    pub witness_index: i32,
    pub maturity: i32,
    pub predicate: HexString,
    pub predicate_data: HexString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputContract {
    pub utxo_id: HexStringUtxoId,
    pub balance_root: HexString256,
    pub state_root: HexString256,
    pub contract_id: HexString256,
}

impl TryFrom<Input> for fuel_tx::Input {
    type Error = ConversionError;

    fn try_from(input: Input) -> Result<fuel_tx::Input, Self::Error> {
        Ok(match input {
            Input::InputCoin(coin) => fuel_tx::Input::Coin {
                utxo_id: coin.utxo_id.into(),
                owner: coin.owner.into(),
                amount: coin.amount.try_into()?,
                color: coin.color.into(),
                witness_index: coin.witness_index.try_into()?,
                maturity: coin.maturity.try_into()?,
                predicate: coin.predicate.into(),
                predicate_data: coin.predicate_data.into(),
            },
            Input::InputContract(contract) => fuel_tx::Input::Contract {
                utxo_id: contract.utxo_id.into(),
                balance_root: contract.balance_root.into(),
                state_root: contract.state_root.into(),
                contract_id: contract.contract_id.into(),
            },
        })
    }
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Output {
    CoinOutput(CoinOutput),
    ContractOutput(ContractOutput),
    WithdrawalOutput(WithdrawalOutput),
    ChangeOutput(ChangeOutput),
    VariableOutput(VariableOutput),
    ContractCreated(ContractCreated),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinOutput {
    pub to: HexString256,
    pub amount: i32,
    pub color: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct WithdrawalOutput {
    pub to: HexString256,
    pub amount: i32,
    pub color: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ChangeOutput {
    pub to: HexString256,
    pub amount: i32,
    pub color: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct VariableOutput {
    pub to: HexString256,
    pub amount: i32,
    pub color: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractOutput {
    pub input_index: i32,
    pub balance_root: HexString256,
    pub state_root: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractCreated {
    contract_id: HexString256,
    state_root: HexString256,
}

impl TryFrom<Output> for fuel_tx::Output {
    type Error = ConversionError;

    fn try_from(value: Output) -> Result<Self, Self::Error> {
        Ok(match value {
            Output::CoinOutput(coin) => Self::Coin {
                to: coin.to.into(),
                amount: coin.amount.try_into()?,
                color: coin.color.into(),
            },
            Output::ContractOutput(contract) => Self::Contract {
                input_index: contract.input_index.try_into()?,
                balance_root: contract.balance_root.into(),
                state_root: contract.state_root.into(),
            },
            Output::WithdrawalOutput(withdrawal) => Self::Withdrawal {
                to: withdrawal.to.into(),
                amount: withdrawal.amount.try_into()?,
                color: withdrawal.color.into(),
            },
            Output::ChangeOutput(change) => Self::Change {
                to: change.to.into(),
                amount: change.amount.try_into()?,
                color: change.color.into(),
            },
            Output::VariableOutput(variable) => Self::Variable {
                to: variable.to.into(),
                amount: variable.amount.try_into()?,
                color: variable.color.into(),
            },
            Output::ContractCreated(contract) => Self::ContractCreated {
                contract_id: contract.contract_id.into(),
                state_root: contract.state_root.into(),
            },
        })
    }
}
