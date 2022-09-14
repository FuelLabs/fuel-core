use crate::schema::{
    contract::Contract,
    scalars::{
        Address,
        AssetId,
        Bytes32,
        U64,
    },
};
use async_graphql::{
    Object,
    Union,
};
use fuel_core_interfaces::common::{
    fuel_asm::Word,
    fuel_tx,
    fuel_types,
};

#[derive(Union)]
pub enum Output {
    Coin(CoinOutput),
    Contract(ContractOutput),
    Message(MessageOutput),
    Change(ChangeOutput),
    Variable(VariableOutput),
    ContractCreated(ContractCreated),
}

pub struct CoinOutput {
    to: fuel_types::Address,
    amount: Word,
    asset_id: fuel_types::AssetId,
}

#[Object]
impl CoinOutput {
    async fn to(&self) -> Address {
        self.to.into()
    }

    async fn amount(&self) -> U64 {
        self.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.asset_id.into()
    }
}

pub struct MessageOutput {
    amount: Word,
    recipient: fuel_types::Address,
}

#[Object]
impl MessageOutput {
    async fn recipient(&self) -> Address {
        self.recipient.into()
    }

    async fn amount(&self) -> U64 {
        self.amount.into()
    }
}

pub struct ChangeOutput(CoinOutput);

#[Object]
impl ChangeOutput {
    async fn to(&self) -> Address {
        self.0.to.into()
    }

    async fn amount(&self) -> U64 {
        self.0.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.0.asset_id.into()
    }
}

pub struct VariableOutput(CoinOutput);

#[Object]
impl VariableOutput {
    async fn to(&self) -> Address {
        self.0.to.into()
    }

    async fn amount(&self) -> U64 {
        self.0.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.0.asset_id.into()
    }
}

pub struct ContractOutput {
    input_index: u8,
    balance_root: fuel_types::Bytes32,
    state_root: fuel_types::Bytes32,
}

#[Object]
impl ContractOutput {
    async fn input_index(&self) -> u8 {
        self.input_index
    }

    async fn balance_root(&self) -> Bytes32 {
        self.balance_root.into()
    }

    async fn state_root(&self) -> Bytes32 {
        self.state_root.into()
    }
}

pub struct ContractCreated {
    contract_id: fuel_types::ContractId,
    state_root: fuel_types::Bytes32,
}

#[Object]
impl ContractCreated {
    async fn contract(&self) -> Contract {
        self.contract_id.into()
    }

    async fn state_root(&self) -> Bytes32 {
        self.state_root.into()
    }
}

impl From<&fuel_tx::Output> for Output {
    fn from(output: &fuel_tx::Output) -> Self {
        match output {
            fuel_tx::Output::Coin {
                to,
                amount,
                asset_id,
            } => Output::Coin(CoinOutput {
                to: *to,
                amount: *amount,
                asset_id: *asset_id,
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
            fuel_tx::Output::Message { recipient, amount } => {
                Output::Message(MessageOutput {
                    recipient: *recipient,
                    amount: *amount,
                })
            }
            fuel_tx::Output::Change {
                to,
                amount,
                asset_id,
            } => Output::Change(ChangeOutput(CoinOutput {
                to: *to,
                amount: *amount,
                asset_id: *asset_id,
            })),
            fuel_tx::Output::Variable {
                to,
                amount,
                asset_id,
            } => Output::Variable(VariableOutput(CoinOutput {
                to: *to,
                amount: *amount,
                asset_id: *asset_id,
            })),
            fuel_tx::Output::ContractCreated {
                contract_id,
                state_root,
            } => Output::ContractCreated(ContractCreated {
                contract_id: *contract_id,
                state_root: *state_root,
            }),
        }
    }
}
