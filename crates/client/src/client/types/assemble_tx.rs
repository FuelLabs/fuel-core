use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
        Transaction,
    },
    fuel_types::canonical::Deserialize,
    services::executor::TransactionExecutionResult,
};

use crate::client::schema::{
    self,
    tx::Destroy,
    ConversionError,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequiredBalance {
    pub asset_id: AssetId,
    pub amount: u64,
    pub account: Account,
    pub change_policy: ChangePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Predicate {
    pub address: Address,
    pub predicate: Vec<u8>,
    pub predicate_data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Account {
    Address(Address),
    Predicate(Predicate),
}

impl Account {
    pub fn owner(&self) -> Address {
        match self {
            Account::Address(address) => *address,
            Account::Predicate(predicate) => predicate.address,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangePolicy {
    Change(Address),
    Destroy,
}

#[derive(Debug, Clone)]
pub struct AssembleTransactionResult {
    pub transaction: Transaction,
    pub status: TransactionExecutionResult,
    pub gas_price: u64,
}

impl TryFrom<schema::tx::AssembleTransactionResult> for AssembleTransactionResult {
    type Error = ConversionError;

    fn try_from(
        value: schema::tx::AssembleTransactionResult,
    ) -> Result<Self, Self::Error> {
        let transaction = Transaction::from_bytes(&value.transaction.raw_payload)
            .map_err(ConversionError::TransactionFromBytesError)?;
        let status = value.status.try_into()?;

        Ok(Self {
            transaction,
            status,
            gas_price: value.gas_price.into(),
        })
    }
}

impl TryFrom<RequiredBalance> for schema::tx::RequiredBalance {
    type Error = ConversionError;

    fn try_from(value: RequiredBalance) -> Result<Self, Self::Error> {
        let asset_id = value.asset_id.into();
        let amount = value.amount.into();
        let account = value.account.into();
        let change_policy = value.change_policy.into();

        Ok(Self {
            asset_id,
            amount,
            account,
            change_policy,
        })
    }
}

impl From<Account> for schema::tx::Account {
    fn from(value: Account) -> Self {
        let (address, predicate) = match value {
            Account::Address(address) => (Some(address.into()), None),
            Account::Predicate(predicate) => (None, Some(predicate.into())),
        };

        Self { address, predicate }
    }
}

impl From<Predicate> for schema::tx::Predicate {
    fn from(value: Predicate) -> Self {
        let predicate_address = value.address.into();
        let predicate = value.predicate.into();
        let predicate_data = value.predicate_data.into();

        Self {
            predicate_address,
            predicate,
            predicate_data,
        }
    }
}

impl From<ChangePolicy> for schema::tx::ChangePolicy {
    fn from(value: ChangePolicy) -> Self {
        let (change, destroy) = match value {
            ChangePolicy::Change(address) => (Some(address.into()), None),
            ChangePolicy::Destroy => (None, Some(Destroy::Destroy)),
        };

        Self { change, destroy }
    }
}
