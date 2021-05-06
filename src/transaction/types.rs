use crate::types::Word;

pub type Color = [u8; 32];
pub type Id = [u8; 32];
pub type Root = [u8; 32];

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Witness {
    data: Vec<u8>,
}

impl From<Vec<u8>> for Witness {
    fn from(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl AsRef<[u8]> for Witness {
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl AsMut<[u8]> for Witness {
    fn as_mut(&mut self) -> &mut [u8] {
        self.data.as_mut()
    }
}

impl Extend<u8> for Witness {
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        self.data.extend(iter);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Input {
    Coin {
        utxo_id: Id,
        owner: Id,
        amount: Word,
        color: Color,
        witness_index: u8,
        maturity: u32,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    },

    Contract {
        utxo_id: Id,
        balance_root: Root,
        state_root: Root,
        contract_id: Id,
    },
}

impl Input {
    pub const fn utxo_id(&self) -> &Id {
        match self {
            Self::Coin { utxo_id, .. } => &utxo_id,
            Self::Contract { utxo_id, .. } => &utxo_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Output {
    Coin {
        to: Id,
        amount: Word,
        color: Color,
    },

    Contract {
        input_index: u8,
        balance_root: Root,
        state_root: Root,
    },

    Withdrawal {
        to: Id,
        amount: Word,
        color: Color,
    },

    Change {
        to: Id,
        amount: Word,
        color: Color,
    },

    Variable {
        to: Id,
        amount: Word,
        color: Color,
    },

    ContractCreated {
        contract_id: Id,
    },
}
