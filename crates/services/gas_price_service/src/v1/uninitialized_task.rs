use fuel_core_services::{RunnableService, ServiceRunner};
use fuel_core_types::fuel_types::BlockHeight;
use fuel_gas_price_algorithm::v1::AlgorithmUpdaterV1;
use crate::common::l2_block_source::FuelL2BlockSource;
use crate::common::updater_metadata::UpdaterMetadata;
use crate::v1::algorithm::SharedV1Algorithm;
use crate::v1::metadata::V1AlgorithmConfig;
use crate::v1::service::GasPriceServiceV1;

pub struct UninitializedTask<L2DataStoreView, GasPriceStore, Metadata, SettingsProvider> {
    pub config: V1AlgorithmConfig,
    pub genesis_block_height: BlockHeight,
    pub settings: SettingsProvider,
    pub gas_price_db: GasPriceStore,
    pub on_chain_db: L2DataStoreView,
    pub block_stream: BoxStream<SharedImportResult>,
    pub(crate) shared_algo: SharedV1Algorithm,
    pub(crate) algo_updater: AlgorithmUpdaterV1,
    pub(crate) metadata_storage: Metadata,
}

impl<L2DataStore, L2DataStoreView, GasPriceStore, Metadata, SettingsProvider>
UninitializedTask<L2DataStoreView, GasPriceStore, Metadata, SettingsProvider>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData,
    Metadata: MetadataStorage,
    SettingsProvider: GasPriceSettingsProvider,
{
    pub fn new(
        config: V1AlgorithmConfig,
        genesis_block_height: BlockHeight,
        settings: SettingsProvider,
        block_stream: BoxStream<SharedImportResult>,
        gas_price_db: GasPriceStore,
        metadata_storage: Metadata,
        on_chain_db: L2DataStoreView,
    ) -> anyhow::Result<Self> {
        let latest_block_height: u32 = on_chain_db
            .latest_view()?
            .latest_height()
            .unwrap_or(genesis_block_height)
            .into();

        let (algo_updater, shared_algo) =
            initialize_algorithm(&config, latest_block_height, &metadata_storage)?;

        let task = Self {
            config,
            genesis_block_height,
            settings,
            gas_price_db,
            on_chain_db,
            block_stream,
            algo_updater,
            shared_algo,
            metadata_storage,
        };
        Ok(task)
    }

    pub fn init(
        mut self,
    ) -> anyhow::Result<GasPriceServiceV1<FuelL2BlockSource<SettingsProvider>, Metadata>>
    {
        let mut first_run = false;
        let latest_block_height: u32 = self
            .on_chain_db
            .latest_view()?
            .latest_height()
            .unwrap_or(self.genesis_block_height)
            .into();

        let maybe_metadata_height = self.gas_price_db.latest_height();
        let metadata_height = if let Some(metadata_height) = maybe_metadata_height {
            metadata_height.into()
        } else {
            first_run = true;
            latest_block_height
        };

        let l2_block_source = FuelL2BlockSource::new(
            self.genesis_block_height,
            self.settings.clone(),
            self.block_stream,
        );

        if BlockHeight::from(latest_block_height) == self.genesis_block_height
            || first_run
        {
            let service = GasPriceServiceV1::new(
                l2_block_source,
                self.metadata_storage,
                self.shared_algo,
                self.algo_updater,
            );
            Ok(service)
        } else {
            if latest_block_height > metadata_height {
                sync_gas_price_db_with_on_chain_storage(
                    &self.settings,
                    &mut self.metadata_storage,
                    &self.on_chain_db,
                    metadata_height,
                    latest_block_height,
                )?;
            }

            let service = GasPriceServiceV1::new(
                l2_block_source,
                self.metadata_storage,
                self.shared_algo,
                self.algo_updater,
            );
            Ok(service)
        }
    }
}

#[async_trait::async_trait]
impl<L2DataStore, L2DataStoreView, GasPriceStore, Metadata, SettingsProvider>
RunnableService
for UninitializedTask<L2DataStoreView, GasPriceStore, Metadata, SettingsProvider>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData,
    Metadata: MetadataStorage,
    SettingsProvider: GasPriceSettingsProvider,
{
    const NAME: &'static str = "GasPriceServiceV0";
    type SharedData = SharedV0Algorithm;
    type Task = GasPriceServiceV1<FuelL2BlockSource<SettingsProvider>, Metadata>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_algo.clone()
    }

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        UninitializedTask::init(self)
    }
}

pub fn initialize_algorithm<Metadata>(
    config: &V1AlgorithmConfig,
    latest_block_height: u32,
    metadata_storage: &Metadata,
) -> GasPriceResult<(AlgorithmUpdaterV1, SharedV1Algorithm)>
where
    Metadata: MetadataStorage,
{
    let min_exec_gas_price = config.min_gas_price;
    let exec_gas_price_change_percent = config.gas_price_change_percent;
    let l2_block_fullness_threshold_percent = config.gas_price_threshold_percent;

    let algorithm_updater;
    if let Some(updater_metadata) = metadata_storage
        .get_metadata(&latest_block_height.into())
        .map_err(|err| GasPriceError::CouldNotInitUpdater(anyhow::anyhow!(err)))?
    {
        let previous_metadata: V0Metadata = updater_metadata.try_into()?;
        algorithm_updater = AlgorithmUpdaterV1::new(
            previous_metadata.new_exec_price,
            min_exec_gas_price,
            exec_gas_price_change_percent,
            previous_metadata.l2_block_height,
            l2_block_fullness_threshold_percent,
        );
    } else {
        algorithm_updater = AlgorithmUpdaterV1::new(
            config.starting_gas_price,
            min_exec_gas_price,
            exec_gas_price_change_percent,
            latest_block_height,
            l2_block_fullness_threshold_percent,
        );
    }

    let shared_algo =
        SharedGasPriceAlgo::new_with_algorithm(algorithm_updater.algorithm());

    Ok((algorithm_updater, shared_algo))
}

fn sync_gas_price_db_with_on_chain_storage<
    L2DataStore,
    L2DataStoreView,
    Metadata,
    SettingsProvider,
>(
    settings: &SettingsProvider,
    metadata_storage: &mut Metadata,
    on_chain_db: &L2DataStoreView,
    metadata_height: u32,
    latest_block_height: u32,
) -> anyhow::Result<()>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    Metadata: MetadataStorage,
    SettingsProvider: GasPriceSettingsProvider,
{
    let metadata = metadata_storage
        .get_metadata(&metadata_height.into())?
        .ok_or(anyhow::anyhow!(
            "Expected metadata to exist for height: {metadata_height}"
        ))?;

    let mut algo_updater = if let UpdaterMetadata::V1(metadata) = metadata {
        Ok(AlgorithmUpdaterV1::new(
            metadata.new_exec_price,
            0,
            0,
            metadata.l2_block_height,
            0,
        ))
    } else {
        Err(anyhow::anyhow!("Expected V0 metadata"))
    }?;

    sync_v0_metadata(
        settings,
        on_chain_db,
        metadata_height,
        latest_block_height,
        &mut algo_updater,
        metadata_storage,
    )?;

    Ok(())
}

fn sync_v0_metadata<L2DataStore, L2DataStoreView, Metadata, SettingsProvider>(
    settings: &SettingsProvider,
    on_chain_db: &L2DataStoreView,
    metadata_height: u32,
    latest_block_height: u32,
    updater: &mut AlgorithmUpdaterV1,
    metadata_storage: &mut Metadata,
) -> anyhow::Result<()>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    Metadata: MetadataStorage,
    SettingsProvider: GasPriceSettingsProvider,
{
    let first = metadata_height.saturating_add(1);
    let view = on_chain_db.latest_view()?;
    for height in first..=latest_block_height {
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

        updater.update_l2_block_data(height, block_gas_used, block_gas_capacity)?;
        let metadata: UpdaterMetadata = updater.clone().into();
        metadata_storage.set_metadata(&metadata)?;
    }

    Ok(())
}

pub fn new_gas_price_service_v1<
    L2DataStore,
    L2DataStoreView,
    GasPriceStore,
    Metadata,
    SettingsProvider,
>(
    config: GasPriceServiceConfig,
    genesis_block_height: BlockHeight,
    settings: SettingsProvider,
    block_stream: BoxStream<SharedImportResult>,
    gas_price_db: GasPriceStore,
    metadata: Metadata,
    on_chain_db: L2DataStoreView,
) -> anyhow::Result<
    ServiceRunner<
        UninitializedTask<L2DataStoreView, GasPriceStore, Metadata, SettingsProvider>,
    >,
>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData,
    SettingsProvider: GasPriceSettingsProvider,
    Metadata: MetadataStorage,
{
    let v0_config = config.v0().ok_or(anyhow::anyhow!("Expected V0 config"))?;
    let gas_price_init = UninitializedTask::new(
        v0_config,
        genesis_block_height,
        settings,
        block_stream,
        gas_price_db,
        metadata,
        on_chain_db,
    )?;
    Ok(ServiceRunner::new(gas_price_init))
}
