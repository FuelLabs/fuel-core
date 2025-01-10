use crate::{
    common::{
        fuel_core_storage_adapter::{
            block_bytes,
            get_block_info,
            mint_values,
            storage::GasPriceColumn,
            GasPriceSettings,
            GasPriceSettingsProvider,
        },
        gas_price_algorithm::SharedGasPriceAlgo,
        l2_block_source::FuelL2BlockSource,
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
        GasPriceServiceConfig,
        GetLatestRecordedHeight,
        GetMetadataStorage,
        L2Data,
        SetLatestRecordedHeight,
        SetMetadataStorage,
    },
    v1::{
        algorithm::SharedV1Algorithm,
        da_source_service::{
            block_committer_costs::BlockCommitterDaBlockCosts,
            service::{
                DaBlockCostsSource,
                DaSourceService,
            },
        },
        metadata::{
            v1_algorithm_from_metadata,
            V1AlgorithmConfig,
            V1Metadata,
        },
        service::{
            initialize_algorithm,
            GasPriceServiceV1,
            LatestGasPrice,
        },
        uninitialized_task::fuel_storage_unrecorded_blocks::{
            AsUnrecordedBlocks,
            FuelStorageUnrecordedBlocks,
        },
    },
};
use anyhow::Error;
use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    Service,
    ServiceRunner,
    State,
    StateWatcher,
};
use fuel_core_storage::{
    kv_store::{
        KeyValueInspect,
        KeyValueMutate,
    },
    not_found,
    transactional::{
        AtomicView,
        Modifiable,
        StorageTransaction,
    },
};
use fuel_core_types::{
    fuel_tx::field::MintAmount,
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use fuel_gas_price_algorithm::v1::{
    AlgorithmUpdaterV1,
    UnrecordedBlocks,
};
use std::{
    sync::Arc,
    time::Duration,
};

pub mod fuel_storage_unrecorded_blocks;

pub struct UninitializedTask<L2DataStoreView, GasPriceStore, DA, SettingsProvider> {
    pub config: V1AlgorithmConfig,
    pub gas_metadata_height: Option<BlockHeight>,
    pub genesis_block_height: BlockHeight,
    pub settings: SettingsProvider,
    pub gas_price_db: GasPriceStore,
    pub on_chain_db: L2DataStoreView,
    pub block_stream: BoxStream<SharedImportResult>,
    pub(crate) shared_algo: SharedV1Algorithm,
    pub(crate) latest_gas_price: LatestGasPrice<u32, u64>,
    pub(crate) algo_updater: AlgorithmUpdaterV1,
    pub(crate) da_source: DA,
}

impl<L2DataStore, L2DataStoreView, AtomicStorage, DA, SettingsProvider>
    UninitializedTask<L2DataStoreView, AtomicStorage, DA, SettingsProvider>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    AtomicStorage: GasPriceServiceAtomicStorage,
    DA: DaBlockCostsSource + 'static,
    SettingsProvider: GasPriceSettingsProvider,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: V1AlgorithmConfig,
        gas_metadata_height: Option<BlockHeight>,
        genesis_block_height: BlockHeight,
        settings: SettingsProvider,
        block_stream: BoxStream<SharedImportResult>,
        gas_price_db: AtomicStorage,
        da_source: DA,
        on_chain_db: L2DataStoreView,
    ) -> anyhow::Result<Self> {
        let latest_block_height: u32 = on_chain_db
            .latest_view()?
            .latest_height()
            .unwrap_or(genesis_block_height)
            .into();
        let latest_gas_price = on_chain_db
            .latest_view()?
            .get_block(&latest_block_height.into())?
            .and_then(|block| {
                let (_, gas_price) = mint_values(&block).ok()?;
                Some(gas_price)
            })
            .unwrap_or(0);
        let gas_price_metadata_height = gas_metadata_height
            .map(|x| x.into())
            .unwrap_or(latest_block_height);

        let (algo_updater, shared_algo) =
            initialize_algorithm(&config, gas_price_metadata_height, &gas_price_db)?;

        let latest_gas_price = LatestGasPrice::new(latest_block_height, latest_gas_price);

        let task = Self {
            config,
            gas_metadata_height,
            genesis_block_height,
            settings,
            gas_price_db,
            on_chain_db,
            block_stream,
            algo_updater,
            latest_gas_price,
            shared_algo,
            da_source,
        };
        Ok(task)
    }

    pub async fn init(
        mut self,
        state_watcher: &StateWatcher,
    ) -> anyhow::Result<
        GasPriceServiceV1<FuelL2BlockSource<SettingsProvider>, DA, AtomicStorage>,
    > {
        let latest_block_height: u32 = self
            .on_chain_db
            .latest_view()?
            .latest_height()
            .unwrap_or(self.genesis_block_height)
            .into();

        let maybe_metadata_height = self.gas_metadata_height;
        let metadata_height = if let Some(metadata_height) = maybe_metadata_height {
            metadata_height.into()
        } else {
            latest_block_height
        };

        let l2_block_source = FuelL2BlockSource::new(
            self.genesis_block_height,
            self.settings.clone(),
            self.block_stream,
        );

        let starting_recorded_height = self
            .gas_price_db
            .get_recorded_height()?
            .unwrap_or(BlockHeight::from(latest_block_height));

        let poll_duration = self
            .config
            .da_poll_interval
            .map(|x| Duration::from_millis(x.into()));
        let latest_l2_height = Arc::new(std::sync::Mutex::new(BlockHeight::new(0)));
        let da_service = DaSourceService::new(
            self.da_source,
            poll_duration,
            latest_l2_height.clone(),
            starting_recorded_height,
        );
        let da_service_runner = ServiceRunner::new(da_service);
        da_service_runner.start_and_await().await?;

        if BlockHeight::from(latest_block_height) == self.genesis_block_height {
            let service = GasPriceServiceV1::new(
                l2_block_source,
                self.shared_algo,
                self.latest_gas_price,
                self.algo_updater,
                da_service_runner,
                self.gas_price_db,
                latest_l2_height,
            );
            Ok(service)
        } else {
            if latest_block_height > metadata_height {
                sync_v1_metadata(
                    &self.settings,
                    &self.on_chain_db,
                    metadata_height,
                    latest_block_height,
                    &mut self.algo_updater,
                    &mut self.gas_price_db,
                    state_watcher,
                )?;
            }

            let service = GasPriceServiceV1::new(
                l2_block_source,
                self.shared_algo,
                self.latest_gas_price,
                self.algo_updater,
                da_service_runner,
                self.gas_price_db,
                latest_l2_height,
            );
            Ok(service)
        }
    }
}

#[async_trait::async_trait]
impl<L2DataStore, L2DataStoreView, AtomicStorage, DA, SettingsProvider> RunnableService
    for UninitializedTask<L2DataStoreView, AtomicStorage, DA, SettingsProvider>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    AtomicStorage: GasPriceServiceAtomicStorage + GasPriceData,
    DA: DaBlockCostsSource + 'static,
    SettingsProvider: GasPriceSettingsProvider + 'static,
{
    const NAME: &'static str = "GasPriceServiceV1";
    type SharedData = (SharedV1Algorithm, LatestGasPrice<u32, u64>);
    type Task = GasPriceServiceV1<FuelL2BlockSource<SettingsProvider>, DA, AtomicStorage>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        (self.shared_algo.clone(), self.latest_gas_price.clone())
    }

    async fn into_task(
        self,
        state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        UninitializedTask::init(self, state_watcher).await
    }
}

fn sync_v1_metadata<L2DataStore, L2DataStoreView, SettingsProvider, AtomicStorage>(
    settings: &SettingsProvider,
    on_chain_db: &L2DataStoreView,
    metadata_height: u32,
    latest_block_height: u32,
    updater: &mut AlgorithmUpdaterV1,
    da_storage: &mut AtomicStorage,
    state_watcher: &StateWatcher,
) -> anyhow::Result<()>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    SettingsProvider: GasPriceSettingsProvider,
    AtomicStorage: GasPriceServiceAtomicStorage,
{
    let first = metadata_height.saturating_add(1);
    let view = on_chain_db.latest_view()?;
    tracing::debug!(
        "Syncing gas price metadata from {} to {}",
        first,
        latest_block_height
    );
    for height in first..=latest_block_height {
        // allows early exit if the service is stopping
        let state = state_watcher.borrow();
        if state.stopping() || state.stopped() {
            return Ok(());
        }

        tracing::info!("Syncing gas price metadata for block {}", height);
        let mut tx = da_storage.begin_transaction()?;
        let block = view
            .get_block(&height.into())?
            .ok_or(not_found!("FullBlock"))?;
        let param_version = block.header().consensus_parameters_version;

        let GasPriceSettings {
            gas_price_factor,
            block_gas_limit,
        } = settings.settings(&param_version)?;
        let block_gas_capacity = block_gas_limit.try_into()?;

        let block_gas_used =
            match get_block_info(&block, gas_price_factor, block_gas_limit)? {
                BlockInfo::GenesisBlock => {
                    Err(anyhow::anyhow!("should not be genesis block"))?
                }
                BlockInfo::Block { gas_used, .. } => gas_used,
            };

        let block_bytes = block_bytes(&block);
        let (fee_gwei, _) = mint_values(&block)?;
        let fee_wei = fee_gwei.saturating_mul(1_000_000_000);
        updater.update_l2_block_data(
            height,
            block_gas_used,
            block_gas_capacity,
            block_bytes,
            fee_wei.into(),
            &mut tx.as_unrecorded_blocks(),
        )?;
        let metadata: UpdaterMetadata = updater.clone().into();
        tx.set_metadata(&metadata)?;
        AtomicStorage::commit_transaction(tx)?;
    }

    Ok(())
}

#[allow(clippy::type_complexity)]
pub fn new_gas_price_service_v1<L2DataStore, AtomicStorage, DA, SettingsProvider>(
    v1_config: V1AlgorithmConfig,
    genesis_block_height: BlockHeight,
    settings: SettingsProvider,
    block_stream: BoxStream<SharedImportResult>,
    gas_price_db: AtomicStorage,
    da_source: DA,
    on_chain_db: L2DataStore,
) -> anyhow::Result<
    ServiceRunner<UninitializedTask<L2DataStore, AtomicStorage, DA, SettingsProvider>>,
>
where
    L2DataStore: AtomicView,
    L2DataStore::LatestView: L2Data,
    AtomicStorage: GasPriceServiceAtomicStorage + GasPriceData,
    SettingsProvider: GasPriceSettingsProvider,
    DA: DaBlockCostsSource,
{
    let metadata_height = gas_price_db.latest_height();
    let gas_price_init = UninitializedTask::new(
        v1_config,
        metadata_height,
        genesis_block_height,
        settings,
        block_stream,
        gas_price_db,
        da_source,
        on_chain_db,
    )?;
    Ok(ServiceRunner::new(gas_price_init))
}
