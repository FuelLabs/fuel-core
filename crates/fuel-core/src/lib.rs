#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use crate::service::genesis::NotifyCancel;
use tokio_util::sync::CancellationToken;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[doc(no_inline)]
pub use fuel_core_chain_config as chain_config;
#[cfg(feature = "p2p")]
#[doc(no_inline)]
pub use fuel_core_p2p as p2p;
#[doc(no_inline)]
pub use fuel_core_producer as producer;
#[cfg(feature = "relayer")]
#[doc(no_inline)]
pub use fuel_core_relayer as relayer;
#[cfg(feature = "p2p")]
#[doc(no_inline)]
pub use fuel_core_sync as sync;
#[doc(no_inline)]
pub use fuel_core_txpool as txpool;
#[doc(no_inline)]
pub use fuel_core_types as types;

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
                #[cfg(unix)]
                tokio::select! {
                    _ = sigterm.recv() => {
                        tracing::info!("Received SIGTERM");
                    }
                    _ = sigint.recv() => {
                        tracing::info!("Received SIGINT");
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

#[async_trait::async_trait]
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
