pub use fuel_tx::ContractId;
pub use fuel_tx::{Transaction, TxId};
use fuel_types::Word;
use std::sync::Arc;

pub type ArcTx = Arc<Transaction>;
pub type GasPrice = Word;
