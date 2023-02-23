use crate::graphql_api::ports::DatabasePort;
mod balance;
mod block;
mod chain;
mod coin;
mod contract;
mod message;
mod subscriptions;
mod tx;

pub trait QueryData:
    Send
    + Sync
    + BalanceQueryData
    + BlockQueryData
    + ChainQueryData
    + CoinQueryData
    + ContractQueryData
    + MessageQueryData
    + MessageProofData
    + TransactionQueryData
{
}
impl<D> QueryData for D 
where D:
     Send
     + Sync
     + BalanceQueryData
     + BlockQueryData
     + ChainQueryData
     + CoinQueryData
     + ContractQueryData
     + MessageQueryData
     + MessageProofData
     + TransactionQueryData
     {}

// TODO: Remove reexporting of everything
pub use balance::*;
pub use block::*;
pub use chain::*;
pub use coin::*;
pub use contract::*;
pub use message::*;
pub(crate) use subscriptions::*;
pub use tx::*;
