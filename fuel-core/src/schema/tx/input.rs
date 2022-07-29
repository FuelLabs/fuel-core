use crate::schema::{
    contract::Contract,
    scalars::{Address, AssetId, Bytes32, ContractId, HexString, MessageId, UtxoId, U64},
};
use async_graphql::{Object, Union};
use fuel_core_interfaces::common::fuel_tx;

#[derive(Union)]
pub enum Input {
    Coin(InputCoin),
    Contract(InputContract),
    Message(InputMessage),
}

pub struct InputCoin {
    utxo_id: UtxoId,
    owner: Address,
    amount: U64,
    asset_id: AssetId,
    witness_index: u8,
    maturity: U64,
    predicate: HexString,
    predicate_data: HexString,
}

#[Object]
impl InputCoin {
    async fn utxo_id(&self) -> UtxoId {
        self.utxo_id
    }

    async fn owner(&self) -> Address {
        self.owner
    }

    async fn amount(&self) -> U64 {
        self.amount
    }

    async fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    async fn witness_index(&self) -> u8 {
        self.witness_index
    }

    async fn maturity(&self) -> U64 {
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
    utxo_id: UtxoId,
    balance_root: Bytes32,
    state_root: Bytes32,
    contract_id: ContractId,
}

#[Object]
impl InputContract {
    async fn utxo_id(&self) -> UtxoId {
        self.utxo_id
    }

    async fn balance_root(&self) -> Bytes32 {
        self.balance_root
    }

    async fn state_root(&self) -> Bytes32 {
        self.state_root
    }

    async fn contract(&self) -> Contract {
        self.contract_id.0.into()
    }
}

pub struct InputMessage {
    message_id: MessageId,
    sender: Address,
    recipient: Address,
    amount: U64,
    nonce: U64,
    owner: Address,
    witness_index: u8,
    data: HexString,
    predicate: HexString,
    predicate_data: HexString,
}

#[Object]
impl InputMessage {
    async fn message_id(&self) -> MessageId {
        self.message_id
    }

    async fn sender(&self) -> Address {
        self.sender
    }

    async fn recipient(&self) -> Address {
        self.recipient
    }

    async fn amount(&self) -> U64 {
        self.amount
    }

    async fn nonce(&self) -> U64 {
        self.nonce
    }

    async fn owner(&self) -> Address {
        self.owner
    }

    async fn witness_index(&self) -> u8 {
        self.witness_index
    }

    async fn data(&self) -> &HexString {
        &self.data
    }

    async fn predicate(&self) -> &HexString {
        &self.predicate
    }

    async fn predicate_data(&self) -> &HexString {
        &self.predicate_data
    }
}

impl From<&fuel_tx::Input> for Input {
    fn from(input: &fuel_tx::Input) -> Self {
        match input {
            fuel_tx::Input::CoinSigned {
                utxo_id,
                owner,
                amount,
                asset_id,
                witness_index,
                maturity,
            } => Input::Coin(InputCoin {
                utxo_id: UtxoId(*utxo_id),
                owner: Address(*owner),
                amount: (*amount).into(),
                asset_id: AssetId(*asset_id),
                witness_index: *witness_index,
                maturity: (*maturity).into(),
                predicate: HexString(Default::default()),
                predicate_data: HexString(Default::default()),
            }),
            fuel_tx::Input::CoinPredicate {
                utxo_id,
                owner,
                amount,
                asset_id,
                maturity,
                predicate,
                predicate_data,
            } => Input::Coin(InputCoin {
                utxo_id: UtxoId(*utxo_id),
                owner: Address(*owner),
                amount: (*amount).into(),
                asset_id: AssetId(*asset_id),
                witness_index: Default::default(),
                maturity: (*maturity).into(),
                predicate: HexString(predicate.clone()),
                predicate_data: HexString(predicate_data.clone()),
            }),
            fuel_tx::Input::Contract {
                utxo_id,
                balance_root,
                state_root,
                contract_id,
            } => Input::Contract(InputContract {
                utxo_id: UtxoId(*utxo_id),
                balance_root: Bytes32(*balance_root),
                state_root: Bytes32(*state_root),
                contract_id: ContractId(*contract_id),
            }),
            fuel_tx::Input::MessageSigned {
                message_id,
                sender,
                recipient,
                amount,
                nonce,
                owner,
                witness_index,
                data,
            } => Input::Message(InputMessage {
                message_id: MessageId(*message_id),
                sender: Address(*sender),
                recipient: Address(*recipient),
                amount: (*amount).into(),
                nonce: (*nonce).into(),
                owner: Address(*owner),
                witness_index: *witness_index,
                data: HexString(data.clone()),
                predicate: HexString(Default::default()),
                predicate_data: HexString(Default::default()),
            }),
            fuel_tx::Input::MessagePredicate {
                message_id,
                sender,
                recipient,
                amount,
                nonce,
                owner,
                data,
                predicate,
                predicate_data,
            } => Input::Message(InputMessage {
                message_id: MessageId(*message_id),
                sender: Address(*sender),
                recipient: Address(*recipient),
                amount: (*amount).into(),
                nonce: (*nonce).into(),
                owner: Address(*owner),
                witness_index: Default::default(),
                data: HexString(data.clone()),
                predicate: HexString(predicate.clone()),
                predicate_data: HexString(predicate_data.clone()),
            }),
        }
    }
}
