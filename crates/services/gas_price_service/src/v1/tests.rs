#![allow(non_snake_case)]
use crate::{
    common::{
        fuel_core_storage_adapter::{
            storage::{
                GasPriceColumn,
                GasPriceMetadata,
            },
            GasPriceSettings,
            GasPriceSettingsProvider,
        },
        l2_block_source::L2BlockSource,
        updater_metadata::UpdaterMetadata,
        utils::{
            BlockInfo,
            Error as GasPriceError,
            Result as GasPriceResult,
        },
    },
    ports::{
        GasPriceData,
        GasPriceServiceAtomicStorage,
        GetLatestRecordedHeight,
        GetMetadataStorage,
        L2Data,
        SetLatestRecordedHeight,
        SetMetadataStorage,
    },
    v1::{
        algorithm::SharedV1Algorithm,
        da_source_service::{
            service::{
                new_da_service,
                DaBlockCostsSource,
            },
            DaBlockCosts,
        },
        metadata::{
            updater_from_config,
            V1AlgorithmConfig,
            V1Metadata,
        },
        service::{
            initialize_algorithm,
            GasPriceServiceV1,
            LatestGasPrice,
        },
        uninitialized_task::{
            fuel_storage_unrecorded_blocks::AsUnrecordedBlocks,
            UninitializedTask,
        },
    },
};
use anyhow::{
    anyhow,
    Result,
};
use fuel_core_services::{
    stream::{
        BoxStream,
        IntoBoxStream,
    },
    RunnableTask,
    Service,
    StateWatcher,
};
use fuel_core_storage::{
    structured_storage::test::InMemoryStorage,
    transactional::{
        AtomicView,
        IntoTransaction,
        StorageTransaction,
        WriteTransaction,
    },
    Result as StorageResult,
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::ConsensusParametersVersion,
    },
    fuel_tx::{
        Mint,
        Transaction,
    },
    fuel_types::BlockHeight,
    services::block_importer::{
        ImportResult,
        SharedImportResult,
    },
};
use fuel_gas_price_algorithm::v1::{
    AlgorithmUpdaterV1,
    Bytes,
    Height,
    UnrecordedBlocks,
};
use std::{
    collections::HashMap,
    num::NonZeroU64,
    ops::Deref,
    sync::{
        atomic::{
            AtomicU32,
            Ordering,
        },
        Arc,
        Mutex,
    },
    time::Duration,
};
use tokio::sync::mpsc::Receiver;

struct FakeL2BlockSource {
    l2_block: Receiver<BlockInfo>,
}

impl L2BlockSource for FakeL2BlockSource {
    async fn get_l2_block(&mut self) -> GasPriceResult<BlockInfo> {
        let block = self.l2_block.recv().await.unwrap();
        Ok(block)
    }
}

struct FakeMetadata {
    inner: Arc<Mutex<Option<UpdaterMetadata>>>,
}

impl FakeMetadata {
    fn empty() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }
}

impl SetMetadataStorage for FakeMetadata {
    fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> GasPriceResult<()> {
        *self.inner.lock().unwrap() = Some(metadata.clone());
        Ok(())
    }
}

impl GetMetadataStorage for FakeMetadata {
    fn get_metadata(&self, _: &BlockHeight) -> GasPriceResult<Option<UpdaterMetadata>> {
        let metadata = self.inner.lock().unwrap().clone();
        Ok(metadata)
    }
}

struct ErroringPersistedData;

impl SetMetadataStorage for ErroringPersistedData {
    fn set_metadata(&mut self, _: &UpdaterMetadata) -> GasPriceResult<()> {
        Err(GasPriceError::CouldNotSetMetadata {
            block_height: Default::default(),
            source_error: anyhow!("boo!"),
        })
    }
}

impl GetMetadataStorage for ErroringPersistedData {
    fn get_metadata(&self, _: &BlockHeight) -> GasPriceResult<Option<UpdaterMetadata>> {
        Err(GasPriceError::CouldNotFetchMetadata {
            source_error: anyhow!("boo!"),
        })
    }
}

impl GetLatestRecordedHeight for ErroringPersistedData {
    fn get_recorded_height(&self) -> GasPriceResult<Option<BlockHeight>> {
        Err(GasPriceError::CouldNotFetchRecordedHeight(anyhow!("boo!")))
    }
}

struct UnimplementedStorageTx;

impl GasPriceServiceAtomicStorage for ErroringPersistedData {
    type Transaction<'a> = UnimplementedStorageTx;

    fn begin_transaction(&mut self) -> GasPriceResult<Self::Transaction<'_>> {
        todo!()
    }

    fn commit_transaction(_transaction: Self::Transaction<'_>) -> GasPriceResult<()> {
        todo!()
    }
}

impl SetMetadataStorage for UnimplementedStorageTx {
    fn set_metadata(&mut self, _metadata: &UpdaterMetadata) -> GasPriceResult<()> {
        unimplemented!()
    }
}

impl GetMetadataStorage for UnimplementedStorageTx {
    fn get_metadata(
        &self,
        _block_height: &BlockHeight,
    ) -> GasPriceResult<Option<UpdaterMetadata>> {
        unimplemented!()
    }
}

impl UnrecordedBlocks for UnimplementedStorageTx {
    fn insert(&mut self, _height: Height, _bytes: Bytes) -> Result<(), String> {
        unimplemented!()
    }

    fn remove(&mut self, _height: &Height) -> Result<Option<Bytes>, String> {
        unimplemented!()
    }
}

impl SetLatestRecordedHeight for UnimplementedStorageTx {
    fn set_recorded_height(&mut self, _bundle_id: BlockHeight) -> GasPriceResult<()> {
        unimplemented!()
    }
}

impl GetLatestRecordedHeight for UnimplementedStorageTx {
    fn get_recorded_height(&self) -> GasPriceResult<Option<BlockHeight>> {
        unimplemented!()
    }
}

impl AsUnrecordedBlocks for UnimplementedStorageTx {
    type Wrapper<'a>
        = UnimplementedStorageTx
    where
        Self: 'a;

    fn as_unrecorded_blocks(&mut self) -> Self::Wrapper<'_> {
        UnimplementedStorageTx
    }
}

struct FakeDABlockCost {
    da_block_costs: Receiver<DaBlockCosts>,
    latest_requested_height: Arc<Mutex<BlockHeight>>,
}

impl FakeDABlockCost {
    fn never_returns() -> Self {
        let (_sender, receiver) = tokio::sync::mpsc::channel(1);
        let block_height = BlockHeight::from(0);
        Self {
            da_block_costs: receiver,
            latest_requested_height: Arc::new(Mutex::new(block_height)),
        }
    }

    fn new(da_block_costs: Receiver<DaBlockCosts>) -> Self {
        let block_height = BlockHeight::from(0);

        Self {
            da_block_costs,
            latest_requested_height: Arc::new(Mutex::new(block_height)),
        }
    }

    fn never_returns_with_handle_to_last_height() -> (Self, Arc<Mutex<BlockHeight>>) {
        let (_sender, receiver) = tokio::sync::mpsc::channel(1);
        let block_height = BlockHeight::from(0);
        let thread_safe_block_height = Arc::new(Mutex::new(block_height));
        let service = Self {
            da_block_costs: receiver,
            latest_requested_height: thread_safe_block_height.clone(),
        };
        (service, thread_safe_block_height)
    }
}

#[async_trait::async_trait]
impl DaBlockCostsSource for FakeDABlockCost {
    async fn request_da_block_costs(
        &mut self,
        latest_recorded_height: &BlockHeight,
    ) -> Result<Vec<DaBlockCosts>> {
        *self.latest_requested_height.lock().unwrap() = *latest_recorded_height;
        let costs = self.da_block_costs.recv().await.unwrap();
        Ok(vec![costs])
    }
}

fn zero_threshold_arbitrary_config() -> V1AlgorithmConfig {
    V1AlgorithmConfig {
        new_exec_gas_price: 100,
        min_exec_gas_price: 0,
        exec_gas_price_change_percent: 10,
        l2_block_fullness_threshold_percent: 0,
        gas_price_factor: NonZeroU64::new(100).unwrap(),
        min_da_gas_price: 0,
        max_da_gas_price: 1,
        max_da_gas_price_change_percent: 0,
        da_p_component: 0,
        da_d_component: 0,
        normal_range_size: 0,
        capped_range_size: 0,
        decrease_range_size: 0,
        block_activity_threshold: 0,
        da_poll_interval: None,
        starting_recorded_height: None,
    }
}

fn arbitrary_metadata() -> V1Metadata {
    V1Metadata {
        new_scaled_exec_price: 100,
        l2_block_height: 0,
        new_scaled_da_gas_price: 0,
        gas_price_factor: NonZeroU64::new(100).unwrap(),
        total_da_rewards: 0,
        latest_known_total_da_cost: 0,
        last_profit: 0,
        second_to_last_profit: 0,
        latest_da_cost_per_byte: 0,
        unrecorded_block_bytes: 0,
    }
}

fn different_arb_config() -> V1AlgorithmConfig {
    V1AlgorithmConfig {
        new_exec_gas_price: 200,
        min_exec_gas_price: 0,
        exec_gas_price_change_percent: 20,
        l2_block_fullness_threshold_percent: 0,
        gas_price_factor: NonZeroU64::new(100).unwrap(),
        min_da_gas_price: 0,
        max_da_gas_price: 1,
        max_da_gas_price_change_percent: 0,
        da_p_component: 0,
        da_d_component: 0,
        normal_range_size: 0,
        capped_range_size: 0,
        decrease_range_size: 0,
        block_activity_threshold: 0,
        da_poll_interval: None,
        starting_recorded_height: None,
    }
}

fn database() -> StorageTransaction<InMemoryStorage<GasPriceColumn>> {
    InMemoryStorage::default().into_transaction()
}

fn gas_price_database_with_metadata(
    metadata: &V1Metadata,
    starting_recorded_height: Option<BlockHeight>,
) -> StorageTransaction<InMemoryStorage<GasPriceColumn>> {
    let mut db = database();
    let mut tx = db.write_transaction();
    let height = metadata.l2_block_height.into();
    let metadata = UpdaterMetadata::V1(metadata.clone());
    tx.storage_as_mut::<GasPriceMetadata>()
        .insert(&height, &metadata)
        .unwrap();

    if let Some(starting_recorded_height) = starting_recorded_height {
        tx.set_recorded_height(starting_recorded_height).unwrap();
    }

    tx.commit().unwrap();
    db
}

#[tokio::test]
async fn next_gas_price__affected_by_new_l2_block() {
    // given
    let l2_block = BlockInfo::Block {
        height: 1,
        gas_used: 60,
        block_gas_capacity: 100,
        block_bytes: 100,
        block_fees: 100,
        gas_price: 100,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: l2_block_receiver,
    };
    let metadata_storage = FakeMetadata::empty();

    let config = zero_threshold_arbitrary_config();
    let height = 0;
    let inner = database();
    let (algo_updater, shared_algo) =
        initialize_algorithm(&config, height, height, &metadata_storage).unwrap();
    let da_source = FakeDABlockCost::never_returns();
    let latest_l2_height = Arc::new(AtomicU32::new(0));
    let recorded_height = BlockHeight::new(0);
    let da_service_runner = new_da_service(
        da_source,
        None,
        Arc::clone(&latest_l2_height),
        recorded_height,
    );

    da_service_runner.start_and_await().await.unwrap();

    let latest_gas_price = LatestGasPrice::new(0, 0);
    let mut service = GasPriceServiceV1::new(
        l2_block_source,
        shared_algo,
        latest_gas_price,
        algo_updater,
        da_service_runner,
        inner,
        latest_l2_height,
        None,
    );

    let read_algo = service.next_block_algorithm();
    let initial = read_algo.next_gas_price();
    let mut watcher = StateWatcher::started();
    tokio::spawn(async move { service.run(&mut watcher).await });

    // when
    l2_block_sender.send(l2_block).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // then
    let new = read_algo.next_gas_price();
    assert_ne!(initial, new);
}

#[tokio::test]
async fn run__new_l2_block_saves_old_metadata() {
    // given
    let height = 1;
    let l2_block = BlockInfo::Block {
        height,
        gas_used: 60,
        block_gas_capacity: 100,
        block_bytes: 100,
        block_fees: 100,
        gas_price: 100,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: l2_block_receiver,
    };

    let config = zero_threshold_arbitrary_config();
    let inner = database();
    let algo_updater = updater_from_config(&config, 0);
    let shared_algo = SharedV1Algorithm::new_with_algorithm(algo_updater.algorithm());
    let da_source = FakeDABlockCost::never_returns();
    let latest_l2_height = Arc::new(AtomicU32::new(0));
    let recorded_height = BlockHeight::new(0);
    let da_service_runner = new_da_service(
        da_source,
        None,
        Arc::clone(&latest_l2_height),
        recorded_height,
    );

    da_service_runner.start_and_await().await.unwrap();

    let latest_gas_price = LatestGasPrice::new(0, 0);
    let mut service = GasPriceServiceV1::new(
        l2_block_source,
        shared_algo,
        latest_gas_price,
        algo_updater,
        da_service_runner,
        inner,
        latest_l2_height,
        None,
    );
    let mut watcher = StateWatcher::started();

    // when
    l2_block_sender.send(l2_block).await.unwrap();
    service.run(&mut watcher).await;

    // then
    let metadata_is_some = service
        .storage_tx_provider()
        .get_metadata(&height.into())
        .unwrap()
        .is_some();
    assert!(metadata_is_some);

    // cleanup
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn run__new_l2_block_updates_latest_gas_price_arc() {
    // given
    let height = 1;
    let gas_price = 40;
    let l2_block = BlockInfo::Block {
        height,
        gas_used: 60,
        block_gas_capacity: 100,
        block_bytes: 100,
        block_fees: 100,
        gas_price,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: l2_block_receiver,
    };

    let config = zero_threshold_arbitrary_config();
    let inner = database();
    let algo_updater = updater_from_config(&config, 0);
    let shared_algo = SharedV1Algorithm::new_with_algorithm(algo_updater.algorithm());
    let da_source = FakeDABlockCost::never_returns();
    let latest_l2_height = Arc::new(AtomicU32::new(0));
    let recorded_height = BlockHeight::new(0);
    let da_service_runner = new_da_service(
        da_source,
        None,
        Arc::clone(&latest_l2_height),
        recorded_height,
    );

    let latest_gas_price = LatestGasPrice::new(0, 0);
    let mut service = GasPriceServiceV1::new(
        l2_block_source,
        shared_algo,
        latest_gas_price.clone(),
        algo_updater,
        da_service_runner,
        inner,
        latest_l2_height,
        None,
    );
    let mut watcher = StateWatcher::started();

    // when
    l2_block_sender.send(l2_block).await.unwrap();
    service.run(&mut watcher).await;

    // then
    let expected = (height, gas_price);
    let actual = latest_gas_price.get();
    assert_eq!(expected, actual);

    // cleanup
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn run__updates_da_service_latest_l2_height() {
    // given
    let l2_height = 10;
    let l2_block = BlockInfo::Block {
        height: l2_height,
        gas_used: 60,
        block_gas_capacity: 100,
        block_bytes: 100,
        block_fees: 100,
        gas_price: 100,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: l2_block_receiver,
    };

    let config = zero_threshold_arbitrary_config();
    let inner = database();
    let mut algo_updater = updater_from_config(&config, 0);
    algo_updater.l2_block_height = l2_height - 1;
    let shared_algo = SharedV1Algorithm::new_with_algorithm(algo_updater.algorithm());
    let da_source = FakeDABlockCost::never_returns();
    let latest_l2_height = Arc::new(AtomicU32::new(0));
    let latest_gas_price = LatestGasPrice::new(0, 0);
    let recorded_height = BlockHeight::new(0);
    let da_service_runner = new_da_service(
        da_source,
        None,
        Arc::clone(&latest_l2_height),
        recorded_height,
    );

    da_service_runner.start_and_await().await.unwrap();

    let mut service = GasPriceServiceV1::new(
        l2_block_source,
        shared_algo,
        latest_gas_price,
        algo_updater,
        da_service_runner,
        inner,
        latest_l2_height,
        None,
    );
    let mut watcher = StateWatcher::started();

    // when
    l2_block_sender.send(l2_block).await.unwrap();
    let _ = service.run(&mut watcher).await;

    // then
    let latest_value = service.latest_l2_block().load(Ordering::SeqCst);
    assert_eq!(latest_value, l2_height);
}

#[derive(Clone)]
struct FakeSettings {
    settings: GasPriceSettings,
}

impl Default for FakeSettings {
    fn default() -> Self {
        Self {
            settings: GasPriceSettings {
                gas_price_factor: 100,
                block_gas_limit: u64::MAX,
            },
        }
    }
}

impl GasPriceSettingsProvider for FakeSettings {
    fn settings(
        &self,
        _param_version: &ConsensusParametersVersion,
    ) -> GasPriceResult<GasPriceSettings> {
        Ok(self.settings.clone())
    }
}

#[derive(Clone)]
struct FakeGasPriceDb {
    height: Option<BlockHeight>,
}

impl FakeGasPriceDb {
    fn new(height: u32) -> Self {
        Self {
            height: Some(height.into()),
        }
    }

    fn empty() -> Self {
        Self { height: None }
    }
}

// GasPriceData + Modifiable + KeyValueInspect<Column = GasPriceColumn>
impl GasPriceData for FakeGasPriceDb {
    fn latest_height(&self) -> Option<BlockHeight> {
        self.height
    }
}

#[derive(Clone)]
struct FakeOnChainDb {
    height: BlockHeight,
    blocks: HashMap<BlockHeight, Block<Transaction>>,
}

impl FakeOnChainDb {
    fn new(height: u32) -> Self {
        Self {
            height: height.into(),
            blocks: HashMap::new(),
        }
    }
}

struct FakeL2Data {
    height: BlockHeight,
    blocks: HashMap<BlockHeight, Block<Transaction>>,
}

impl FakeL2Data {
    fn new(
        height: BlockHeight,
        blocks: HashMap<BlockHeight, Block<Transaction>>,
    ) -> Self {
        Self { height, blocks }
    }
}

impl L2Data for FakeL2Data {
    fn latest_height(&self) -> StorageResult<BlockHeight> {
        Ok(self.height)
    }

    fn get_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<Block<Transaction>>> {
        Ok(self.blocks.get(height).cloned())
    }
}
impl AtomicView for FakeOnChainDb {
    type LatestView = FakeL2Data;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(FakeL2Data::new(self.height, self.blocks.clone()))
    }
}

fn empty_block_stream() -> BoxStream<SharedImportResult> {
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> = vec![];
    tokio_stream::iter(blocks).into_boxed()
}

#[tokio::test]
async fn uninitialized_task__new__if_exists_already_reload_old_values_with_overrides() {
    // given
    let original_metadata = arbitrary_metadata();
    let different_config = different_arb_config();
    let descaleed_exec_price =
        original_metadata.new_scaled_exec_price / original_metadata.gas_price_factor;
    assert_ne!(different_config.new_exec_gas_price, descaleed_exec_price);
    let different_l2_block = 0;
    let settings = FakeSettings::default();
    let block_stream = empty_block_stream();
    let on_chain_db = FakeOnChainDb::new(different_l2_block);
    let da_cost_source = FakeDABlockCost::never_returns();
    let inner = gas_price_database_with_metadata(&original_metadata, None);
    // when
    let service = UninitializedTask::new(
        different_config.clone(),
        None,
        0.into(),
        settings,
        block_stream,
        inner,
        da_cost_source,
        on_chain_db,
    )
    .unwrap();

    // then
    let UninitializedTask { algo_updater, .. } = service;
    algo_updater_matches_values_from_old_metadata(
        algo_updater.clone(),
        original_metadata.clone(),
    );
    algo_updater_override_values_match(algo_updater, different_config);
}

fn algo_updater_matches_values_from_old_metadata(
    mut algo_updater: AlgorithmUpdaterV1,
    original_metadata: V1Metadata,
) {
    let V1Metadata {
        new_scaled_exec_price: original_new_scaled_exec_price,
        l2_block_height: original_l2_block_height,
        new_scaled_da_gas_price: original_new_scaled_da_gas_price,
        gas_price_factor: original_gas_price_factor,
        total_da_rewards: original_total_da_rewards,
        latest_known_total_da_cost: original_latest_known_total_da_cost,
        last_profit: original_last_profit,
        second_to_last_profit: original_second_to_last_profit,
        latest_da_cost_per_byte: original_latest_da_cost_per_byte,
        unrecorded_block_bytes: original_unrecorded_block_bytes,
    } = original_metadata;
    assert_eq!(
        algo_updater.new_scaled_exec_price,
        original_new_scaled_exec_price
    );
    assert_eq!(algo_updater.l2_block_height, original_l2_block_height);
    assert_eq!(
        algo_updater.new_scaled_da_gas_price,
        original_new_scaled_da_gas_price
    );
    assert_eq!(algo_updater.gas_price_factor, original_gas_price_factor);
    assert_eq!(algo_updater.total_da_rewards, original_total_da_rewards);
    assert_eq!(
        algo_updater.latest_known_total_da_cost,
        original_latest_known_total_da_cost
    );
    assert_eq!(algo_updater.last_profit, original_last_profit);
    assert_eq!(
        algo_updater.second_to_last_profit,
        original_second_to_last_profit
    );
    assert_eq!(
        algo_updater.latest_da_cost_per_byte,
        original_latest_da_cost_per_byte
    );
    assert_eq!(
        algo_updater.unrecorded_blocks_bytes,
        original_unrecorded_block_bytes
    );
}

fn algo_updater_override_values_match(
    algo_updater: AlgorithmUpdaterV1,
    config: V1AlgorithmConfig,
) {
    assert_eq!(algo_updater.min_exec_gas_price, config.min_exec_gas_price);
    assert_eq!(
        algo_updater.exec_gas_price_change_percent,
        config.exec_gas_price_change_percent
    );
    assert_eq!(
        algo_updater.l2_block_fullness_threshold_percent,
        config.l2_block_fullness_threshold_percent.into()
    );
    assert_eq!(algo_updater.gas_price_factor, config.gas_price_factor);
    assert_eq!(algo_updater.min_da_gas_price, config.min_da_gas_price);
    assert_eq!(
        algo_updater.max_da_gas_price_change_percent,
        config.max_da_gas_price_change_percent
    );
    assert_eq!(algo_updater.da_p_component, config.da_p_component);
    assert_eq!(algo_updater.da_d_component, config.da_d_component);
}

#[tokio::test]
async fn uninitialized_task__new__if_no_metadata_found_use_latest_l2_height() {
    // given
    let different_config = different_arb_config();
    let l2_block = 1234;
    let settings = FakeSettings::default();
    let block_stream = empty_block_stream();
    let on_chain_db = FakeOnChainDb::new(l2_block);
    let da_cost_source = FakeDABlockCost::never_returns();
    let inner = database();
    // when
    let service = UninitializedTask::new(
        different_config.clone(),
        None,
        0.into(),
        settings,
        block_stream,
        inner,
        da_cost_source,
        on_chain_db,
    )
    .unwrap();

    // then
    let UninitializedTask { algo_updater, .. } = service;
    let algo_height = algo_updater.l2_block_height;
    assert_eq!(algo_height, l2_block);
}

#[tokio::test]
async fn uninitialized_task__new__should_fail_if_cannot_fetch_metadata() {
    // given
    let config = zero_threshold_arbitrary_config();
    let different_l2_block = 1231;
    let erroring_persisted_data = ErroringPersistedData;
    let settings = FakeSettings::default();
    let block_stream = empty_block_stream();
    let on_chain_db = FakeOnChainDb::new(different_l2_block);
    let da_cost_source = FakeDABlockCost::never_returns();

    // when
    let res = UninitializedTask::new(
        config,
        None,
        0.into(),
        settings,
        block_stream,
        erroring_persisted_data,
        da_cost_source,
        on_chain_db,
    );

    // then
    let is_err = res.is_err();
    assert!(is_err);
}

#[tokio::test]
async fn uninitialized_task__init__starts_da_service_with_recorded_height_in_storage() {
    // given
    let block_height = 100;
    let recorded_height: u32 = 200;
    let original_metadata = arbitrary_metadata();

    let mut different_config = different_arb_config();
    let descaleed_exec_price =
        original_metadata.new_scaled_exec_price / original_metadata.gas_price_factor;
    assert_ne!(different_config.new_exec_gas_price, descaleed_exec_price);
    let different_l2_block = 0;
    let settings = FakeSettings::default();
    let block_stream = empty_block_stream();
    let on_chain_db = FakeOnChainDb::new(different_l2_block);
    let (da_cost_source, latest_requested_recorded_height) =
        FakeDABlockCost::never_returns_with_handle_to_last_height();
    let gas_price_db = gas_price_database_with_metadata(
        &original_metadata,
        Some(recorded_height.into()),
    );

    different_config.da_poll_interval = Some(Duration::from_millis(1));

    let service = UninitializedTask::new(
        different_config.clone(),
        Some(block_height.into()),
        0.into(),
        settings,
        block_stream,
        gas_price_db,
        da_cost_source,
        on_chain_db,
    )
    .unwrap();

    // when
    let _gas_price_service = service.init(&StateWatcher::started()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // then
    let actual = latest_requested_recorded_height.lock().unwrap();
    let expected = BlockHeight::new(recorded_height);
    assert_eq!(*actual, expected);
}

fn arb_block() -> Block<Transaction> {
    let mut block = Block::default();
    let mint = Mint::default();
    block.transactions_mut().push(mint.into());
    block
}

#[tokio::test]
async fn uninitialized_task__init__if_metadata_behind_l2_height_then_sync() {
    // given
    let metadata_height = 100;
    let l2_height = 200;
    let config = zero_threshold_arbitrary_config();

    let metadata = V1Metadata {
        new_scaled_exec_price: 100,
        l2_block_height: metadata_height,
        new_scaled_da_gas_price: 0,
        gas_price_factor: NonZeroU64::new(100).unwrap(),
        total_da_rewards: 0,
        latest_known_total_da_cost: 0,
        last_profit: 0,
        second_to_last_profit: 0,
        latest_da_cost_per_byte: 0,
        unrecorded_block_bytes: 0,
    };
    let gas_price_db = gas_price_database_with_metadata(&metadata, None);
    let mut onchain_db = FakeOnChainDb::new(l2_height);
    for height in 1..=l2_height {
        let block = arb_block();
        onchain_db.blocks.insert(BlockHeight::from(height), block);
    }

    let service = UninitializedTask::new(
        config,
        Some(metadata_height.into()),
        0.into(),
        FakeSettings::default(),
        empty_block_stream(),
        gas_price_db,
        FakeDABlockCost::never_returns(),
        onchain_db.clone(),
    )
    .unwrap();

    // when
    let gas_price_service = service.init(&StateWatcher::started()).await.unwrap();

    // then
    // sleep to allow the service to sync
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let on_chain_height = u32::from(onchain_db.height);
    let algo_updater_height = gas_price_service.algorithm_updater().l2_block_height;

    assert_eq!(on_chain_height, algo_updater_height);
}

#[tokio::test]
async fn uninitialized_task__init__sets_initial_storage_height_to_match_l2_height_if_none(
) {
    // given
    let metadata_height = 100;
    let l2_height = 200;
    let config = zero_threshold_arbitrary_config();

    let metadata = V1Metadata {
        new_scaled_exec_price: 100,
        l2_block_height: metadata_height,
        new_scaled_da_gas_price: 0,
        gas_price_factor: NonZeroU64::new(100).unwrap(),
        total_da_rewards: 0,
        latest_known_total_da_cost: 0,
        last_profit: 0,
        second_to_last_profit: 0,
        latest_da_cost_per_byte: 0,
        unrecorded_block_bytes: 0,
    };
    let gas_price_db = gas_price_database_with_metadata(&metadata, None);
    let mut onchain_db = FakeOnChainDb::new(l2_height);
    for height in 1..=l2_height {
        let block = arb_block();
        onchain_db.blocks.insert(BlockHeight::from(height), block);
    }

    let service = UninitializedTask::new(
        config,
        Some(metadata_height.into()),
        0.into(),
        FakeSettings::default(),
        empty_block_stream(),
        gas_price_db,
        FakeDABlockCost::never_returns(),
        onchain_db.clone(),
    )
    .unwrap();

    // when
    let gas_price_service = service.init(&StateWatcher::started()).await.unwrap();

    // then
    // sleep to allow the service to sync
    tokio::time::sleep(Duration::from_millis(100)).await;

    let initial_recorded_height = gas_price_service.initial_recorded_height();
    assert_eq!(initial_recorded_height, Some(l2_height.into()));
}
