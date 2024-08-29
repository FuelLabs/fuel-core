use fuel_core_consensus_module::{
    block_verifier::Verifier,
    RelayerConsensusConfig,
};
use fuel_core_importer::ImporterResult;
use fuel_core_poa::{
    ports::BlockSigner,
    signer::SignMode,
};
use fuel_core_services::stream::BoxStream;
#[cfg(feature = "p2p")]
use fuel_core_types::services::p2p::peer_reputation::AppScore;
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
    },
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
    tai64::Tai64,
};
use fuel_core_upgradable_executor::executor::Executor;
use std::sync::Arc;

use crate::{
    database::{
        database_description::relayer::Relayer,
        Database,
    },
    service::{
        sub_services::{
            BlockProducerService,
            TxPoolSharedState,
        },
        vm_pool::MemoryPool,
    },
};

pub mod block_importer;
pub mod consensus_module;
pub mod consensus_parameters_provider;
pub mod executor;
pub mod fuel_gas_price_provider;
pub mod gas_price_adapters;
pub mod graphql_api;
pub mod import_result_provider;
#[cfg(feature = "p2p")]
pub mod p2p;
pub mod producer;
#[cfg(feature = "relayer")]
pub mod relayer;
#[cfg(feature = "p2p")]
pub mod sync;
pub mod txpool;

#[derive(Debug, Clone)]
pub struct ConsensusParametersProvider {
    shared_state: consensus_parameters_provider::SharedState,
}

impl ConsensusParametersProvider {
    pub fn new(shared_state: consensus_parameters_provider::SharedState) -> Self {
        Self { shared_state }
    }
}

#[derive(Debug, Clone)]
pub struct StaticGasPrice {
    pub gas_price: u64,
}

impl StaticGasPrice {
    pub fn new(gas_price: u64) -> Self {
        Self { gas_price }
    }
}

#[derive(Clone)]
pub struct PoAAdapter {
    shared_state: Option<fuel_core_poa::service::SharedState>,
}

#[derive(Clone)]
pub struct TxPoolAdapter {
    service: TxPoolSharedState,
}

impl TxPoolAdapter {
    pub fn new(service: TxPoolSharedState) -> Self {
        Self { service }
    }
}

#[derive(Clone)]
pub struct TransactionsSource {
    txpool: TxPoolSharedState,
    _block_height: BlockHeight,
}

impl TransactionsSource {
    pub fn new(txpool: TxPoolSharedState, block_height: BlockHeight) -> Self {
        Self {
            txpool,
            _block_height: block_height,
        }
    }
}

#[derive(Clone)]
pub struct ExecutorAdapter {
    pub executor: Arc<Executor<Database, Database<Relayer>>>,
}

impl ExecutorAdapter {
    pub fn new(
        database: Database,
        relayer_database: Database<Relayer>,
        config: fuel_core_upgradable_executor::config::Config,
    ) -> Self {
        let executor = Executor::new(database, relayer_database, config);
        Self {
            executor: Arc::new(executor),
        }
    }
}

#[derive(Clone)]
pub struct VerifierAdapter {
    pub block_verifier: Arc<Verifier<Database>>,
}

#[derive(Clone)]
pub struct ConsensusAdapter {
    pub block_verifier: Arc<Verifier<Database>>,
    pub config: RelayerConsensusConfig,
    pub maybe_relayer: MaybeRelayerAdapter,
}

impl ConsensusAdapter {
    pub fn new(
        block_verifier: VerifierAdapter,
        config: RelayerConsensusConfig,
        maybe_relayer: MaybeRelayerAdapter,
    ) -> Self {
        Self {
            block_verifier: block_verifier.block_verifier,
            config,
            maybe_relayer,
        }
    }
}

#[derive(Clone)]
pub struct MaybeRelayerAdapter {
    #[cfg(feature = "relayer")]
    pub relayer_synced: Option<fuel_core_relayer::SharedState<Database<Relayer>>>,
    #[cfg(feature = "relayer")]
    pub da_deploy_height: fuel_core_types::blockchain::primitives::DaBlockHeight,
}

#[derive(Clone)]
pub struct BlockProducerAdapter {
    pub block_producer: Arc<BlockProducerService>,
}

#[derive(Clone)]
pub struct BlockImporterAdapter {
    pub block_importer:
        Arc<fuel_core_importer::Importer<Database, ExecutorAdapter, VerifierAdapter>>,
}

impl BlockImporterAdapter {
    pub fn events(&self) -> BoxStream<ImporterResult> {
        use futures::StreamExt;
        fuel_core_services::stream::IntoBoxStream::into_boxed(
            tokio_stream::wrappers::BroadcastStream::new(self.block_importer.subscribe())
                .filter_map(|r| futures::future::ready(r.ok())),
        )
    }

    pub fn events_shared_result(&self) -> BoxStream<SharedImportResult> {
        use futures::StreamExt;
        fuel_core_services::stream::IntoBoxStream::into_boxed(
            tokio_stream::wrappers::BroadcastStream::new(self.block_importer.subscribe())
                .filter_map(|r| futures::future::ready(r.ok()))
                .map(|r| r.shared_result),
        )
    }
}

pub struct FuelBlockSigner {
    mode: SignMode,
}
impl FuelBlockSigner {
    pub fn new(mode: SignMode) -> Self {
        Self { mode }
    }
}

#[async_trait::async_trait]
impl BlockSigner for FuelBlockSigner {
    async fn seal_block(&self, block: &Block) -> anyhow::Result<Consensus> {
        self.mode.seal_block(block).await
    }

    fn is_available(&self) -> bool {
        self.mode.is_available()
    }
}

#[cfg(feature = "p2p")]
#[derive(Clone)]
pub struct P2PAdapter {
    service: Option<fuel_core_p2p::service::SharedState>,
    peer_report_config: PeerReportConfig,
}

#[cfg(feature = "p2p")]
#[derive(Clone)]
pub struct PeerReportConfig {
    pub successful_block_import: AppScore,
    pub missing_block_headers: AppScore,
    pub bad_block_header: AppScore,
    pub missing_transactions: AppScore,
    pub invalid_transactions: AppScore,
}

#[cfg(not(feature = "p2p"))]
#[derive(Default, Clone)]
pub struct P2PAdapter;

#[cfg(feature = "p2p")]
impl P2PAdapter {
    pub fn new(
        service: Option<fuel_core_p2p::service::SharedState>,
        peer_report_config: PeerReportConfig,
    ) -> Self {
        Self {
            service,
            peer_report_config,
        }
    }
}

#[cfg(not(feature = "p2p"))]
impl P2PAdapter {
    pub fn new() -> Self {
        Default::default()
    }
}

#[derive(Clone)]
pub struct SharedMemoryPool {
    memory_pool: MemoryPool,
}

impl SharedMemoryPool {
    pub fn new(number_of_instances: usize) -> Self {
        Self {
            memory_pool: MemoryPool::new(number_of_instances),
        }
    }
}

pub struct SystemTime;

impl fuel_core_poa::ports::GetTime for SystemTime {
    fn now(&self) -> Tai64 {
        Tai64::now()
    }
}
