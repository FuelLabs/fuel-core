use crate::client::schema::{
    contract::ContractIdFragment,
    schema,
    tx::{tests::transparent_receipt::Receipt, TransactionStatus, TxIdArgs},
    Address, AssetId, Bytes32, ConnectionArgs, ConversionError, HexString, MessageId, PageInfo,
    Salt, TransactionId, UtxoId, U64,
};
use core::convert::{TryFrom, TryInto};
use fuel_tx::StorageSlot;
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
    pub edges: Vec<TransactionEdge>,
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
    pub gas_limit: U64,
    pub gas_price: U64,
    pub id: TransactionId,
    pub input_asset_ids: Vec<AssetId>,
    pub input_contracts: Vec<ContractIdFragment>,
    pub inputs: Vec<Input>,
    pub is_script: bool,
    pub outputs: Vec<Output>,
    pub maturity: U64,
    pub receipts_root: Option<Bytes32>,
    pub status: Option<TransactionStatus>,
    pub witnesses: Vec<HexString>,
    pub receipts: Option<Vec<Receipt>>,
    pub script: Option<HexString>,
    pub script_data: Option<HexString>,
    pub salt: Option<Salt>,
    pub storage_slots: Option<Vec<HexString>>,
    pub bytecode_witness_index: Option<i32>,
    pub bytecode_length: Option<U64>,
}

impl TryFrom<Transaction> for fuel_vm::prelude::Transaction {
    type Error = ConversionError;

    fn try_from(tx: Transaction) -> Result<Self, Self::Error> {
        Ok(match tx.is_script {
            true => Self::Script {
                gas_price: tx.gas_price.into(),
                gas_limit: tx.gas_limit.into(),
                maturity: tx.maturity.into(),
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
                gas_price: tx.gas_price.into(),
                gas_limit: tx.gas_limit.into(),
                maturity: tx.maturity.into(),
                bytecode_length: tx
                    .bytecode_length
                    .ok_or_else(|| ConversionError::MissingField("bytecode_length".to_string()))?
                    .into(),
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
                            fuel_types::Bytes32::try_from(key)
                                .map_err(|_| ConversionError::BytesLength)?,
                            fuel_types::Bytes32::try_from(value)
                                .map_err(|_| ConversionError::BytesLength)?,
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
    InputMessage(InputMessage),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputCoin {
    pub utxo_id: UtxoId,
    pub owner: Address,
    pub amount: U64,
    pub asset_id: AssetId,
    pub witness_index: i32,
    pub maturity: U64,
    pub predicate: HexString,
    pub predicate_data: HexString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputContract {
    pub utxo_id: UtxoId,
    pub balance_root: Bytes32,
    pub state_root: Bytes32,
    pub contract: ContractIdFragment,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputMessage {
    message_id: MessageId,
    sender: Address,
    recipient: Address,
    amount: U64,
    nonce: U64,
    owner: Address,
    witness_index: i32,
    data: HexString,
    predicate: HexString,
    predicate_data: HexString,
}

impl TryFrom<Input> for fuel_tx::Input {
    type Error = ConversionError;

    fn try_from(input: Input) -> Result<fuel_tx::Input, Self::Error> {
        Ok(match input {
            Input::InputCoin(coin) => {
                if coin.predicate.0 .0.is_empty() {
                    fuel_tx::Input::CoinSigned {
                        utxo_id: coin.utxo_id.into(),
                        owner: coin.owner.into(),
                        amount: coin.amount.into(),
                        asset_id: coin.asset_id.into(),
                        witness_index: coin.witness_index.try_into()?,
                        maturity: coin.maturity.into(),
                    }
                } else {
                    fuel_tx::Input::CoinPredicate {
                        utxo_id: coin.utxo_id.into(),
                        owner: coin.owner.into(),
                        amount: coin.amount.into(),
                        asset_id: coin.asset_id.into(),
                        maturity: coin.maturity.into(),
                        predicate: coin.predicate.into(),
                        predicate_data: coin.predicate_data.into(),
                    }
                }
            }
            Input::InputContract(contract) => fuel_tx::Input::Contract {
                utxo_id: contract.utxo_id.into(),
                balance_root: contract.balance_root.into(),
                state_root: contract.state_root.into(),
                contract_id: contract.contract.id.into(),
            },
            Input::InputMessage(message) => {
                if message.predicate.0 .0.is_empty() {
                    fuel_tx::Input::MessageSigned {
                        message_id: message.message_id.into(),
                        sender: message.sender.into(),
                        recipient: message.recipient.into(),
                        amount: message.amount.into(),
                        nonce: message.nonce.into(),
                        owner: message.owner.into(),
                        witness_index: message.witness_index.try_into()?,
                        data: message.data.into(),
                    }
                } else {
                    fuel_tx::Input::MessagePredicate {
                        message_id: message.message_id.into(),
                        sender: message.sender.into(),
                        recipient: message.recipient.into(),
                        amount: message.amount.into(),
                        nonce: message.nonce.into(),
                        owner: message.owner.into(),
                        data: message.data.into(),
                        predicate: message.predicate.into(),
                        predicate_data: message.predicate_data.into(),
                    }
                }
            }
        })
    }
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Output {
    CoinOutput(CoinOutput),
    ContractOutput(ContractOutput),
    MessageOutput(MessageOutput),
    ChangeOutput(ChangeOutput),
    VariableOutput(VariableOutput),
    ContractCreated(ContractCreated),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinOutput {
    pub to: Address,
    pub amount: U64,
    pub asset_id: AssetId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct MessageOutput {
    pub recipient: Address,
    pub amount: U64,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ChangeOutput {
    pub to: Address,
    pub amount: U64,
    pub asset_id: AssetId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct VariableOutput {
    pub to: Address,
    pub amount: U64,
    pub asset_id: AssetId,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractOutput {
    pub input_index: i32,
    pub balance_root: Bytes32,
    pub state_root: Bytes32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractCreated {
    contract: ContractIdFragment,
    state_root: Bytes32,
}

impl TryFrom<Output> for fuel_tx::Output {
    type Error = ConversionError;

    fn try_from(value: Output) -> Result<Self, Self::Error> {
        Ok(match value {
            Output::CoinOutput(coin) => Self::Coin {
                to: coin.to.into(),
                amount: coin.amount.into(),
                asset_id: coin.asset_id.into(),
            },
            Output::ContractOutput(contract) => Self::Contract {
                input_index: contract.input_index.try_into()?,
                balance_root: contract.balance_root.into(),
                state_root: contract.state_root.into(),
            },
            Output::MessageOutput(message) => Self::Message {
                recipient: message.recipient.into(),
                amount: message.amount.into(),
            },
            Output::ChangeOutput(change) => Self::Change {
                to: change.to.into(),
                amount: change.amount.into(),
                asset_id: change.asset_id.into(),
            },
            Output::VariableOutput(variable) => Self::Variable {
                to: variable.to.into(),
                amount: variable.amount.into(),
                asset_id: variable.asset_id.into(),
            },
            Output::ContractCreated(contract) => Self::ContractCreated {
                contract_id: contract.contract.id.into(),
                state_root: contract.state_root.into(),
            },
        })
    }
}
