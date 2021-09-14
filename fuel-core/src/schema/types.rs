use crate::schema::scalars::{HexString, HexString256};
use async_graphql::{Object, Union};
use fuel_asm::Word;
use std::ops::Deref;

#[derive(Union)]
pub enum Input {
    Coin(Coin),
    Contract(Contract),
}

pub struct Coin {
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
impl Coin {
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

pub struct Contract {
    utxo_id: HexString256,
    balance_root: HexString256,
    state_root: HexString256,
    contract_id: HexString256,
}

#[Object]
impl Contract {
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
            } => Input::Coin(Coin {
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
            } => Input::Contract(Contract {
                utxo_id: HexString256(*utxo_id.deref()),
                balance_root: HexString256(*balance_root.deref()),
                state_root: HexString256(*state_root.deref()),
                contract_id: HexString256(*contract_id.deref()),
            }),
        }
    }
}
