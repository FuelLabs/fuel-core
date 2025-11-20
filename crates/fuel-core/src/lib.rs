#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use futures::{
    StreamExt,
    stream::FuturesUnordered,
};
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

use fuel_core_block_aggregator_api as _;

pub mod fuel_core_graphql_api {
    pub use crate::graphql_api::*;
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();

#[derive(Clone)]
pub struct ShutdownListener {
    pub token: CancellationToken,
}

pub struct SignalKind {
    _inner: tokio::signal::unix::SignalKind,
    variant: SignalVariant,
}

impl SignalKind {
    pub fn new(variant: SignalVariant) -> Self {
        let _inner = match variant {
            SignalVariant::SIGTERM => tokio::signal::unix::SignalKind::terminate(),
            SignalVariant::SIGINT => tokio::signal::unix::SignalKind::interrupt(),
            SignalVariant::SIGABRT => {
                tokio::signal::unix::SignalKind::from_raw(libc::SIGABRT)
            }
            SignalVariant::SIGHUP => tokio::signal::unix::SignalKind::hangup(),
            SignalVariant::SIGQUIT => tokio::signal::unix::SignalKind::quit(),
        };

        Self { _inner, variant }
    }

    pub fn decompose(self) -> (tokio::signal::unix::SignalKind, SignalVariant) {
        (self._inner, self.variant)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SignalVariant {
    SIGTERM,
    SIGINT,
    SIGABRT,
    SIGHUP,
    SIGQUIT,
}

impl ShutdownListener {
    pub fn spawn() -> Self {
        let token = CancellationToken::new();
        {
            let token = token.clone();
            tokio::spawn(async move {
                #[cfg(unix)]
                {
                    let signal_kinds: Vec<_> = vec![
                        SignalVariant::SIGINT,
                        SignalVariant::SIGTERM,
                        SignalVariant::SIGABRT,
                        SignalVariant::SIGHUP,
                        SignalVariant::SIGQUIT,
                    ]
                    .into_iter()
                    .map(SignalKind::new)
                    .collect();

                    let signals_with_variants: Vec<_> = signal_kinds
                        .into_iter()
                        .filter_map(|signal_kind| {
                            let (signal_kind, variant) = signal_kind.decompose();
                            match tokio::signal::unix::signal(signal_kind) {
                                Ok(signal) => {
                                    tracing::info!("Registered signal handler for {variant:?}");
                                    Some((signal, variant))
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to register signal handler for {variant:?}: {e}");
                                    None
                                }
                            }
                        })
                        .collect();

                    let mut signal_futs: FuturesUnordered<_> = signals_with_variants
                        .into_iter()
                        .map(|(mut signal, variant)| async move {
                            signal.recv().await;
                            variant
                        })
                        .collect();

                    let variant = signal_futs.next().await;
                    tracing::error!("Received shutdown signal: {variant:?}");
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
