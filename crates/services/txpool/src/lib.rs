#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use fuel_core_types::{
    services::txpool::{
        ArcPoolTx,
        TransactionStatus,
    },
    tai64::Tai64,
};
use std::{
    ops::Deref,
    time::Duration,
};

pub mod config;
mod containers;
pub mod error;
pub mod ports;
pub mod service;
mod transaction_selector;
pub mod txpool;
pub mod types;

#[cfg(any(test, feature = "test-helpers"))]
pub mod mock_db;
#[cfg(any(test, feature = "test-helpers"))]
pub use mock_db::MockDb;

pub use config::Config;
pub use error::{
    Error,
    Result,
};
pub use service::{
    new_service,
    Service,
};
pub use txpool::TxPool;

#[cfg(any(test, feature = "test-helpers"))]
pub(crate) mod test_helpers;

#[cfg(test)]
fuel_core_trace::enable_tracing!();

/// Information of a transaction fetched from the txpool.
#[derive(Debug, Clone)]
pub struct TxInfo {
    tx: ArcPoolTx,
    submitted_time: Duration,
    creation_instant: tokio::time::Instant,
}

#[allow(missing_docs)]
impl TxInfo {
    pub fn new(tx: ArcPoolTx) -> Self {
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Now is bellow of the `UNIX_EPOCH`");

        Self {
            tx,
            submitted_time: since_epoch,
            creation_instant: tokio::time::Instant::now(),
        }
    }

    pub fn tx(&self) -> &ArcPoolTx {
        &self.tx
    }

    pub fn submitted_time(&self) -> Duration {
        self.submitted_time
    }

    pub fn created(&self) -> tokio::time::Instant {
        self.creation_instant
    }
}

impl Deref for TxInfo {
    type Target = ArcPoolTx;
    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}

impl From<TxInfo> for TransactionStatus {
    fn from(tx_info: TxInfo) -> Self {
        Self::Submitted {
            time: Tai64::from_unix(tx_info.submitted_time.as_secs() as i64),
        }
    }
}
