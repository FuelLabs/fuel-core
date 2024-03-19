use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::TransactionStatus,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::serde_as;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct TxStatusConfig {
    pub id: TxId,
    pub status: TransactionStatus,
}
