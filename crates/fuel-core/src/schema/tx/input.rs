use crate::schema::scalars::{
    Address,
    AssetId,
    Bytes32,
    ContractId,
    HexString,
    Nonce,
    TxPointer,
    UtxoId,
    U16,
    U64,
};
use async_graphql::{
    Object,
    Union,
};
use fuel_core_types::fuel_tx;

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
    tx_pointer: TxPointer,
    witness_index: u16,
    predicate_gas_used: U64,
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

    async fn tx_pointer(&self) -> TxPointer {
        self.tx_pointer
    }

    async fn witness_index(&self) -> u16 {
        self.witness_index
    }

    async fn predicate_gas_used(&self) -> U64 {
        self.predicate_gas_used
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
    tx_pointer: TxPointer,
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

    async fn tx_pointer(&self) -> TxPointer {
        self.tx_pointer
    }

    async fn contract_id(&self) -> ContractId {
        self.contract_id
    }
}

pub struct InputMessage {
    sender: Address,
    recipient: Address,
    amount: U64,
    nonce: Nonce,
    witness_index: u16,
    predicate_gas_used: U64,
    data: HexString,
    predicate: HexString,
    predicate_data: HexString,
}

#[Object]
impl InputMessage {
    async fn sender(&self) -> Address {
        self.sender
    }

    async fn recipient(&self) -> Address {
        self.recipient
    }

    async fn amount(&self) -> U64 {
        self.amount
    }

    async fn nonce(&self) -> Nonce {
        self.nonce
    }

    async fn witness_index(&self) -> U16 {
        self.witness_index.into()
    }

    async fn predicate_gas_used(&self) -> U64 {
        self.predicate_gas_used
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
            fuel_tx::Input::CoinSigned(fuel_tx::input::coin::CoinSigned {
                utxo_id,
                owner,
                amount,
                asset_id,
                tx_pointer,
                witness_index,
                ..
            }) => Input::Coin(InputCoin {
                utxo_id: UtxoId(*utxo_id),
                owner: Address(*owner),
                amount: (*amount).into(),
                asset_id: AssetId(*asset_id),
                tx_pointer: TxPointer(*tx_pointer),
                witness_index: *witness_index,
                predicate_gas_used: 0.into(),
                predicate: HexString(Default::default()),
                predicate_data: HexString(Default::default()),
            }),
            fuel_tx::Input::CoinPredicate(fuel_tx::input::coin::CoinPredicate {
                utxo_id,
                owner,
                amount,
                asset_id,
                tx_pointer,
                predicate,
                predicate_data,
                predicate_gas_used,
                ..
            }) => Input::Coin(InputCoin {
                utxo_id: UtxoId(*utxo_id),
                owner: Address(*owner),
                amount: (*amount).into(),
                asset_id: AssetId(*asset_id),
                tx_pointer: TxPointer(*tx_pointer),
                witness_index: Default::default(),
                predicate_gas_used: (*predicate_gas_used).into(),
                predicate: HexString(predicate.clone()),
                predicate_data: HexString(predicate_data.clone()),
            }),
            fuel_tx::Input::Contract(contract) => Input::Contract(contract.into()),
            fuel_tx::Input::MessageCoinSigned(
                fuel_tx::input::message::MessageCoinSigned {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    witness_index,
                    ..
                },
            ) => Input::Message(InputMessage {
                sender: Address(*sender),
                recipient: Address(*recipient),
                amount: (*amount).into(),
                nonce: Nonce(*nonce),
                witness_index: *witness_index,
                data: HexString(Default::default()),
                predicate_gas_used: 0.into(),
                predicate: HexString(Default::default()),
                predicate_data: HexString(Default::default()),
            }),
            fuel_tx::Input::MessageCoinPredicate(
                fuel_tx::input::message::MessageCoinPredicate {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    predicate,
                    predicate_data,
                    predicate_gas_used,
                    ..
                },
            ) => Input::Message(InputMessage {
                sender: Address(*sender),
                recipient: Address(*recipient),
                amount: (*amount).into(),
                nonce: Nonce(*nonce),
                witness_index: Default::default(),
                predicate_gas_used: (*predicate_gas_used).into(),
                data: HexString(Default::default()),
                predicate: HexString(predicate.clone()),
                predicate_data: HexString(predicate_data.clone()),
            }),
            fuel_tx::Input::MessageDataSigned(
                fuel_tx::input::message::MessageDataSigned {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    witness_index,
                    data,
                    ..
                },
            ) => Input::Message(InputMessage {
                sender: Address(*sender),
                recipient: Address(*recipient),
                amount: (*amount).into(),
                nonce: Nonce(*nonce),
                witness_index: *witness_index,
                predicate_gas_used: 0.into(),
                data: HexString(data.clone()),
                predicate: HexString(Default::default()),
                predicate_data: HexString(Default::default()),
            }),
            fuel_tx::Input::MessageDataPredicate(
                fuel_tx::input::message::MessageDataPredicate {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    data,
                    predicate,
                    predicate_data,
                    predicate_gas_used,
                    ..
                },
            ) => Input::Message(InputMessage {
                sender: Address(*sender),
                recipient: Address(*recipient),
                amount: (*amount).into(),
                nonce: Nonce(*nonce),
                witness_index: Default::default(),
                predicate_gas_used: (*predicate_gas_used).into(),
                data: HexString(data.clone()),
                predicate: HexString(predicate.clone()),
                predicate_data: HexString(predicate_data.clone()),
            }),
        }
    }
}

impl From<&fuel_tx::input::contract::Contract> for InputContract {
    fn from(value: &fuel_tx::input::contract::Contract) -> Self {
        let fuel_tx::input::contract::Contract {
            utxo_id,
            balance_root,
            state_root,
            tx_pointer,
            contract_id,
        } = value;
        InputContract {
            utxo_id: UtxoId(*utxo_id),
            balance_root: Bytes32(*balance_root),
            state_root: Bytes32(*state_root),
            tx_pointer: TxPointer(*tx_pointer),
            contract_id: ContractId(*contract_id),
        }
    }
}
