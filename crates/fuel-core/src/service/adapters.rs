use crate::{
    database::{
        database_description::relayer::Relayer,
        Database,
    },
    fuel_core_graphql_api::ports::GasPriceEstimate,
    service::{
        sub_services::{
            BlockProducerService,
            TxPoolSharedState,
        },
        vm_pool::MemoryPool,
    },
};
use fuel_core_consensus_module::{
    block_verifier::Verifier,
    RelayerConsensusConfig,
};
use fuel_core_executor::executor::OnceTransactionsSource;
use fuel_core_gas_price_service::v1::service::LatestGasPrice;
use fuel_core_importer::ImporterResult;
use fuel_core_poa::ports::BlockSigner;
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::transactional::Changes;
use fuel_core_txpool::{
    ports::GasPriceProvider as TxPoolGasPriceProvider,
    BorrowedTxPool,
};
#[cfg(feature = "p2p")]
use fuel_core_types::services::p2p::peer_reputation::AppScore;
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::{
            poa::PoAConsensus,
            Consensus,
        },
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::{
        block_importer::SharedImportResult,
        block_producer::Components,
        executor::{
            Result as ExecutorResult,
            UncommittedResult,
        },
    },
    signer::SignMode,
    tai64::Tai64,
};
use fuel_core_upgradable_executor::executor::Executor;
use std::sync::Arc;

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
#[cfg(feature = "shared-sequencer")]
pub mod shared_sequencer;
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

#[cfg(test)]
mod universal_gas_price_provider_tests {
    #![allow(non_snake_case)]

    use super::*;
    use proptest::proptest;

    fn _worst_case__correctly_calculates_value(
        gas_price: u64,
        starting_height: u32,
        block_horizon: u32,
        percentage: u16,
    ) {
        // given
        let subject =
            UniversalGasPriceProvider::new(starting_height, gas_price, percentage);

        // when
        let target_height = starting_height.saturating_add(block_horizon);
        let estimated = subject.worst_case_gas_price(target_height.into()).unwrap();

        // then
        let mut actual = gas_price;

        for _ in 0..block_horizon {
            let change_amount =
                actual.saturating_mul(percentage as u64).saturating_div(100);
            actual = actual.saturating_add(change_amount);
        }

        assert!(estimated >= actual);
    }

    proptest! {
        #[test]
        fn worst_case_gas_price__correctly_calculates_value(
            gas_price: u64,
            starting_height: u32,
            block_horizon in 0..10_000u32,
            percentage: u16,
        ) {
            _worst_case__correctly_calculates_value(
                gas_price,
                starting_height,
                block_horizon,
                percentage,
            );
        }
    }

    proptest! {
        #[test]
        fn worst_case_gas_price__never_overflows(
            gas_price: u64,
            starting_height: u32,
            block_horizon in 0..10_000u32,
            percentage: u16
        ) {
            // given
            let subject = UniversalGasPriceProvider::new(starting_height, gas_price, percentage);

            // when
            let target_height = starting_height.saturating_add(block_horizon);

            let _ = subject.worst_case_gas_price(target_height.into());

            // then
            // doesn't panic with an overflow
        }
    }

    fn _next_gas_price__correctly_calculates_value(
        gas_price: u64,
        starting_height: u32,
        percentage: u16,
    ) {
        // given
        let subject =
            UniversalGasPriceProvider::new(starting_height, gas_price, percentage);

        // when
        let estimated = subject.next_gas_price();

        // then
        let change_amount = gas_price
            .saturating_mul(percentage as u64)
            .saturating_div(100);
        let actual = gas_price.saturating_add(change_amount);

        assert!(estimated >= actual);
    }

    proptest! {
        #[test]
        fn next_gas_price__correctly_calculates_value(
            gas_price: u64,
            starting_height: u32,
            percentage: u16,
        ) {
            _next_gas_price__correctly_calculates_value(
                gas_price,
                starting_height,
                percentage,
            )
        }
    }
}

/// Allows communication from other service with more recent gas price data
/// `Height` refers to the height of the block at which the gas price was last updated
/// `GasPrice` refers to the gas price at the last updated block
#[derive(Debug)]
pub struct UniversalGasPriceProvider<Height, GasPrice> {
    /// Shared state of latest gas price data
    latest_gas_price: LatestGasPrice<Height, GasPrice>,
    /// The max percentage the gas price can increase per block
    percentage: u16,
}

impl<Height, GasPrice> Clone for UniversalGasPriceProvider<Height, GasPrice> {
    fn clone(&self) -> Self {
        Self {
            latest_gas_price: self.latest_gas_price.clone(),
            percentage: self.percentage,
        }
    }
}

impl<Height, GasPrice> UniversalGasPriceProvider<Height, GasPrice> {
    #[cfg(test)]
    pub fn new(height: Height, price: GasPrice, percentage: u16) -> Self {
        let latest_gas_price = LatestGasPrice::new(height, price);
        Self {
            latest_gas_price,
            percentage,
        }
    }

    pub fn new_from_inner(
        inner: LatestGasPrice<Height, GasPrice>,
        percentage: u16,
    ) -> Self {
        Self {
            latest_gas_price: inner,
            percentage,
        }
    }
}

impl<Height: Copy, GasPrice: Copy> UniversalGasPriceProvider<Height, GasPrice> {
    fn get_height_and_gas_price(&self) -> (Height, GasPrice) {
        self.latest_gas_price.get()
    }
}

impl UniversalGasPriceProvider<u32, u64> {
    pub fn inner_next_gas_price(&self) -> u64 {
        let (_, latest_price) = self.get_height_and_gas_price();
        let percentage = self.percentage;

        let change = latest_price
            .saturating_mul(percentage as u64)
            .saturating_div(100);

        latest_price.saturating_add(change)
    }
}

impl TxPoolGasPriceProvider for UniversalGasPriceProvider<u32, u64> {
    fn next_gas_price(&self) -> fuel_core_txpool::GasPrice {
        self.inner_next_gas_price()
    }
}

impl GasPriceEstimate for UniversalGasPriceProvider<u32, u64> {
    fn worst_case_gas_price(&self, height: BlockHeight) -> Option<u64> {
        let (best_height, best_gas_price) = self.get_height_and_gas_price();
        let percentage = self.percentage;

        let worst = cumulative_percentage_change(
            best_gas_price,
            best_height,
            percentage as u64,
            height.into(),
        );
        Some(worst)
    }
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn cumulative_percentage_change(
    start_gas_price: u64,
    best_height: u32,
    percentage: u64,
    target_height: u32,
) -> u64 {
    let blocks = target_height.saturating_sub(best_height) as f64;
    let percentage_as_decimal = percentage as f64 / 100.0;
    let multiple = (1.0f64 + percentage_as_decimal).powf(blocks);
    let mut approx = start_gas_price as f64 * multiple;
    // Account for rounding errors and take a slightly higher value
    // Around the `ROUNDING_ERROR_CUTOFF` the rounding errors will cause the estimate to be too low.
    // We increase by `ROUNDING_ERROR_COMPENSATION` to account for this.
    // This is an unlikely situation in practice, but we want to guarantee that the actual
    // gas price is always equal or less than the estimate given here
    const ROUNDING_ERROR_CUTOFF: f64 = 16948547188989277.0;
    if approx > ROUNDING_ERROR_CUTOFF {
        const ROUNDING_ERROR_COMPENSATION: f64 = 2000.0;
        approx += ROUNDING_ERROR_COMPENSATION;
    }
    // `f64` over `u64::MAX` are cast to `u64::MAX`
    approx.ceil() as u64
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

pub struct TransactionsSource {
    tx_pool: BorrowedTxPool,
    minimum_gas_price: u64,
}

impl TransactionsSource {
    pub fn new(minimum_gas_price: u64, tx_pool: BorrowedTxPool) -> Self {
        Self {
            tx_pool,
            minimum_gas_price,
        }
    }
}

#[derive(Clone)]
pub struct ExecutorAdapter {
    pub(crate) executor: Arc<Executor<Database, Database<Relayer>>>,
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

    pub fn produce_without_commit_from_vector(
        &self,
        component: Components<Vec<Transaction>>,
    ) -> ExecutorResult<UncommittedResult<Changes>> {
        let new_components = Components {
            header_to_produce: component.header_to_produce,
            transactions_source: OnceTransactionsSource::new(
                component.transactions_source,
            ),
            gas_price: component.gas_price,
            coinbase_recipient: component.coinbase_recipient,
        };

        self.executor
            .produce_without_commit_with_source(new_components)
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
        let block_hash = block.id();
        let message = block_hash.into_message();
        let signature = self.mode.sign_message(message).await?;
        Ok(Consensus::PoA(PoAConsensus::new(signature)))
    }

    fn is_available(&self) -> bool {
        self.mode.is_available()
    }
}

#[cfg(feature = "shared-sequencer")]
#[async_trait::async_trait]
impl fuel_core_shared_sequencer::ports::Signer for FuelBlockSigner {
    async fn sign(
        &self,
        data: &[u8],
    ) -> anyhow::Result<fuel_core_types::fuel_crypto::Signature> {
        Ok(self.mode.sign(data).await?)
    }

    fn public_key(&self) -> cosmrs::crypto::PublicKey {
        let pubkey = self
            .mode
            .verifying_key()
            .expect("Invalid public key")
            .expect("Public key not available");
        cosmrs::crypto::PublicKey::from(pubkey)
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
