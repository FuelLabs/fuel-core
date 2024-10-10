//! This crate manage the verification, storage, organisation and selection of the transactions for the network.
//! A transaction in Fuel has inputs and outputs. These inputs are outputs of previous transactions.
//! In a case where one of the input is an output of a transaction that has not been executed in a committed block (transaction still in the pool),
//! then the new transaction is considered dependent on that transaction.
//!
//! If a transaction has a dependency, it cannot be selected in a block until the dependent transaction has been selected.
//! A transaction can have a dependency per input and this dependency transaction can also have its own dependencies.
//! This creates a dependency tree between transactions inside the pool which can be very costly to compute for insertion/deletion etc...
//! In order to avoid too much cost, the transaction pool only allow a maximum of transaction inside a dependency chain.
//! There is others limits on the pool that prevent its size to grow too much: maximum gas in the pool, maximum bytes in the pool, maximum number of transactions in the pool.
//! The pool also implements a TTL for the transactions, if a transaction is not selected in a block after a certain time, it is removed from the pool.
//!
//! All the transactions ordered by their ratio of gas/tip to be selected in a block.
//! It's possible that a transaction is not profitable enough to be selected for now and so either it will be selected later or it will be removed from the pool.
//! In order to make a transaction more likely to be selected, it's needed to submit a new collidng transaction (see below) with a higher tip/gas ratio.
//!
//! When a transaction is inserted it's possible that it use same inputs as one or multiple transactions already in the pool: this is what we call a collision.
//! The pool has to choose which transaction to keep and which to remove.
//! The pool will always try to maximize the number of transactions that can be selected in the next block and so
//! during collision resolution it will prioritize transactions without dependencies.
//! In a collision case, the transaction is considered a conflict and can be inserted under certain conditions :
//! - The transaction has dependencies:
//!     - Can collide only with one other transaction. So, the user can submit
//!       the same transaction with a higher tip but not merge one or more
//!         transactions into one.
//!     - A new transaction can be accepted if its profitability is higher than
//!         the collided subtree's.
//! - A transaction doesn't have dependencies:
//!     - A new transaction can be accepted if its profitability is higher
//!         than the collided subtrees'.
//!
//! The pool provides a way to subscribe for updates on a transaction status.
//! It usually stream one or two messages:
//! - If the insertion of the transaction fails, you can expect only one message with the error.
//! - If the transaction is inserted, you can expect two messages: one with the validation of the insertion and one when the transaction is selected in a block.

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

mod collision_manager;
pub mod config;
pub mod error;
mod pool;
pub mod ports;
mod selection_algorithms;
mod service;
mod shared_state;
mod storage;
mod tx_status_stream;
mod update_sender;

type GasPrice = Word;

#[cfg(test)]
mod tests;
#[cfg(test)]
fuel_core_trace::enable_tracing!();

use fuel_core_types::fuel_asm::Word;
pub use selection_algorithms::Constraints;
pub use service::{
    new_service,
    Service,
};
pub use shared_state::{
    BorrowedTxPool,
    SharedState,
};
pub use tx_status_stream::TxStatusMessage;
