use crate::schema::{
    contract::Contract,
    scalars::{Address, AssetId, Bytes32, ContractId, HexString, UtxoId, U64},
};
use async_graphql::{Object, Union};
use fuel_asm::Word;

#[derive(Union)]
pub enum Input {
    Coin(InputCoin),
    Contract(InputContract),
}

pub struct InputCoin {
    utxo_id: UtxoId,
    owner: Address,
    amount: Word,
    asset_id: AssetId,
    witness_index: u8,
    maturity: Word,
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
        self.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    async fn witness_index(&self) -> u8 {
        self.witness_index
    }

    async fn maturity(&self) -> U64 {
        self.maturity.into()
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
                amount: *amount,
                asset_id: AssetId(*asset_id),
                witness_index: *witness_index,
                maturity: *maturity,
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
                amount: *amount,
                asset_id: AssetId(*asset_id),
                witness_index: Default::default(),
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
                utxo_id: UtxoId(*utxo_id),
                balance_root: Bytes32(*balance_root),
                state_root: Bytes32(*state_root),
                contract_id: ContractId(*contract_id),
            }),
        }
    }
}
