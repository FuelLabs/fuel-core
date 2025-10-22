#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[cfg(test)]
use tracing_subscriber as _;

use crate::service::genesis::NotifyCancel;
use tokio_util::sync::CancellationToken;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[doc(no_inline)]
pub use fuel_core_chain_config as chain_config;
#[cfg(feature = "p2p")]
#[doc(no_inline)]
pub use fuel_core_p2p as p2p;
#[cfg(feature = "parallel-executor")]
#[doc(no_inline)]
pub use fuel_core_parallel_executor as parallel_executor;
#[doc(no_inline)]
pub use fuel_core_producer as producer;
#[cfg(feature = "relayer")]
#[doc(no_inline)]
pub use fuel_core_relayer as relayer;
#[cfg(feature = "p2p")]
#[doc(no_inline)]
pub use fuel_core_sync as sync;
#[doc(no_inline)]
pub use fuel_core_tx_status_manager as tx_status_manager;
#[doc(no_inline)]
pub use fuel_core_txpool as txpool;
#[doc(no_inline)]
pub use fuel_core_types as types;
#[doc(no_inline)]
pub use fuel_core_upgradable_executor as upgradable_executor;

pub mod coins_query;
pub mod combined_database;
pub mod database;
pub mod executor;
pub mod model;
#[cfg(all(feature = "p2p", feature = "test-helpers"))]
pub mod p2p_test_helpers;
pub mod query;
pub mod schema;
pub mod service;
pub mod state;

// In the future this module will be a separate crate for `fuel-core-graphql-api`.
mod graphql_api;

pub mod fuel_core_graphql_api {
    pub use crate::graphql_api::*;
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();

#[derive(Clone)]
pub struct ShutdownListener {
    pub token: CancellationToken,
}

impl ShutdownListener {
    pub fn spawn() -> Self {
        let token = CancellationToken::new();
        {
            let token = token.clone();
            tokio::spawn(async move {
                let mut sigterm = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::terminate(),
                )?;
                let mut sigint = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::interrupt(),
                )?;
                let mut sigsegv = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::from_raw(libc::SIGSEGV),
                )?;
                let mut sigkill = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::from_raw(libc::SIGKILL),
                )?;
                let mut sigabrt = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::from_raw(libc::SIGABRT),
                )?;
                let mut sighup = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::hangup()
                )?;
                #[cfg(unix)]
                tokio::select! {
                    _ = sigterm.recv() => {
                        tracing::error!("Received SIGTERM");
                    }
                    _ = sigint.recv() => {
                        tracing::error!("Received SIGINT");
                    }
                    _ = sigsegv.recv() => {
                        tracing::error!("Received SIGSEGV");
                    }
                    _ = sigkill.recv() => {
                        tracing::error!("Received SIGKILL");
                    }
                    _ = sigabrt.recv() => {
                        tracing::error!("Received SIGABRT");
                    }
                    _ = sighup.recv() => {
                        tracing::error!("Received SIGHUP");
                    }
                }
                #[cfg(not(unix))]
                {
                    tokio::signal::ctrl_c().await?;
                    tracing::info!("Received ctrl_c");
                }
                token.cancel();
                tokio::io::Result::Ok(())
            });
        }
        Self { token }
    }
}

impl NotifyCancel for ShutdownListener {
    async fn wait_until_cancelled(&self) -> anyhow::Result<()> {
        self.token.cancelled().await;
        Ok(())
    }

    fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

impl combined_database::ShutdownListener for ShutdownListener {
    fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}
