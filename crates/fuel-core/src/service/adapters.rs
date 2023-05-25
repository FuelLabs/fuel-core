use crate::database::Database;
use fuel_core_consensus_module::block_verifier::Verifier;
use fuel_core_txpool::service::SharedState as TxPoolSharedState;
use std::sync::Arc;

pub mod block_importer;
pub mod consensus_module;
pub mod executor;
pub mod graphql_api;
#[cfg(feature = "p2p")]
pub mod p2p;
pub mod producer;
#[cfg(feature = "p2p")]
pub mod sync;
pub mod txpool;

#[derive(Clone)]
pub struct PoAAdapter {
    shared_state: Option<fuel_core_poa::service::SharedState>,
}

#[derive(Clone)]
pub struct TxPoolAdapter {
    service: TxPoolSharedState<P2PAdapter, Database>,
}

impl TxPoolAdapter {
    pub fn new(service: TxPoolSharedState<P2PAdapter, Database>) -> Self {
        Self { service }
    }
}

#[derive(Clone)]
pub struct ExecutorAdapter {
    pub relayer: MaybeRelayerAdapter,
    pub config: Arc<fuel_core_executor::Config>,
}

#[derive(Clone)]
pub struct VerifierAdapter {
    pub block_verifier: Arc<Verifier<Database, MaybeRelayerAdapter>>,
}

#[derive(Clone)]
pub struct MaybeRelayerAdapter {
    pub database: Database,
    #[cfg(feature = "relayer")]
    pub relayer_synced: Option<fuel_core_relayer::SharedState<Database>>,
    #[cfg(feature = "relayer")]
    pub da_deploy_height: fuel_core_types::blockchain::primitives::DaBlockHeight,
}

#[derive(Clone)]
pub struct BlockProducerAdapter {
    pub block_producer: Arc<fuel_core_producer::Producer<Database>>,
}

#[derive(Clone)]
pub struct BlockImporterAdapter {
    pub block_importer:
        Arc<fuel_core_importer::Importer<Database, ExecutorAdapter, VerifierAdapter>>,
}

#[cfg(feature = "p2p")]
#[derive(Clone)]
pub struct P2PAdapter {
    service: Option<fuel_core_p2p::service::SharedState>,
}

#[cfg(not(feature = "p2p"))]
#[derive(Default, Clone)]
pub struct P2PAdapter;

#[cfg(feature = "p2p")]
impl P2PAdapter {
    pub fn new(service: Option<fuel_core_p2p::service::SharedState>) -> Self {
        Self { service }
    }
}

#[cfg(not(feature = "p2p"))]
impl P2PAdapter {
    pub fn new() -> Self {
        Default::default()
    }
}
