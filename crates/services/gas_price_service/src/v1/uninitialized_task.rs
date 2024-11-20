use crate::{
    common::{
        fuel_core_storage_adapter::{
            block_bytes,
            get_block_info,
            mint_values,
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
        GasPriceServiceConfig,
        L2Data,
        MetadataStorage,
    },
    v1::{
        algorithm::SharedV1Algorithm,
        da_source_service::service::{
            DaBlockCostsSource,
            DaSourceService,
        },
        metadata::{
            v1_algorithm_from_metadata,
            V1AlgorithmConfig,
            V1Metadata,
        },
        service::{
            initialize_algorithm,
            GasPriceServiceV1,
        },
    },
};
use anyhow::Error;
use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_storage::{
    not_found,
    transactional::AtomicView,
};
use fuel_core_types::{
    fuel_tx::field::MintAmount,
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use fuel_gas_price_algorithm::v1::AlgorithmUpdaterV1;

pub struct UninitializedTask<
    L2DataStoreView,
    GasPriceStore,
    Metadata,
    DA,
    SettingsProvider,
> {
    pub config: V1AlgorithmConfig,
    pub genesis_block_height: BlockHeight,
    pub settings: SettingsProvider,
    pub gas_price_db: GasPriceStore,
    pub on_chain_db: L2DataStoreView,
    pub block_stream: BoxStream<SharedImportResult>,
    pub(crate) shared_algo: SharedV1Algorithm,
    pub(crate) algo_updater: AlgorithmUpdaterV1,
    pub(crate) metadata_storage: Metadata,
    pub(crate) da_source: DA,
}

impl<L2DataStore, L2DataStoreView, GasPriceStore, Metadata, DA, SettingsProvider>
    UninitializedTask<L2DataStoreView, GasPriceStore, Metadata, DA, SettingsProvider>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData,
    Metadata: MetadataStorage,
    DA: DaBlockCostsSource,
    SettingsProvider: GasPriceSettingsProvider,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: V1AlgorithmConfig,
        genesis_block_height: BlockHeight,
        settings: SettingsProvider,
        block_stream: BoxStream<SharedImportResult>,
        gas_price_db: GasPriceStore,
        metadata_storage: Metadata,
        da_source: DA,
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
            da_source,
        };
        Ok(task)
    }

    pub fn init(
        mut self,
    ) -> anyhow::Result<
        GasPriceServiceV1<FuelL2BlockSource<SettingsProvider>, Metadata, DA>,
    > {
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

        // TODO: Add to config
        // https://github.com/FuelLabs/fuel-core/issues/2140
        let poll_interval = None;
        let da_service = DaSourceService::new(self.da_source, poll_interval);

        if BlockHeight::from(latest_block_height) == self.genesis_block_height
            || first_run
        {
            let service = GasPriceServiceV1::new(
                l2_block_source,
                self.metadata_storage,
                self.shared_algo,
                self.algo_updater,
                da_service,
            );
            Ok(service)
        } else {
            if latest_block_height > metadata_height {
                sync_gas_price_db_with_on_chain_storage(
                    &self.settings,
                    &self.config,
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
                da_service,
            );
            Ok(service)
        }
    }
}

#[async_trait::async_trait]
impl<L2DataStore, L2DataStoreView, GasPriceStore, Metadata, DA, SettingsProvider>
    RunnableService
    for UninitializedTask<L2DataStoreView, GasPriceStore, Metadata, DA, SettingsProvider>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData,
    Metadata: MetadataStorage,
    DA: DaBlockCostsSource,
    SettingsProvider: GasPriceSettingsProvider,
{
    const NAME: &'static str = "GasPriceServiceV1";
    type SharedData = SharedV1Algorithm;
    type Task = GasPriceServiceV1<FuelL2BlockSource<SettingsProvider>, Metadata, DA>;
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

fn sync_gas_price_db_with_on_chain_storage<
    L2DataStore,
    L2DataStoreView,
    Metadata,
    SettingsProvider,
>(
    settings: &SettingsProvider,
    config: &V1AlgorithmConfig,
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

    let metadata = match metadata {
        UpdaterMetadata::V1(metadata) => metadata,
        UpdaterMetadata::V0(metadata) => {
            V1Metadata::construct_from_v0_metadata(metadata, config)?
        }
    };
    let mut algo_updater = v1_algorithm_from_metadata(metadata, config);

    sync_v1_metadata(
        settings,
        on_chain_db,
        metadata_height,
        latest_block_height,
        &mut algo_updater,
        metadata_storage,
    )?;

    Ok(())
}

fn sync_v1_metadata<L2DataStore, L2DataStoreView, Metadata, SettingsProvider>(
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

        let block_bytes = block_bytes(&block);
        let (fee_wei, _) = mint_values(&block)?;
        updater.update_l2_block_data(
            height,
            block_gas_used,
            block_gas_capacity,
            block_bytes,
            fee_wei.into(),
        )?;
        let metadata: UpdaterMetadata = updater.clone().into();
        metadata_storage.set_metadata(&metadata)?;
    }

    Ok(())
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn new_gas_price_service_v1<
    L2DataStore,
    L2DataStoreView,
    GasPriceStore,
    Metadata,
    DA,
    SettingsProvider,
>(
    config: GasPriceServiceConfig,
    genesis_block_height: BlockHeight,
    settings: SettingsProvider,
    block_stream: BoxStream<SharedImportResult>,
    gas_price_db: GasPriceStore,
    metadata: Metadata,
    da_source: DA,
    on_chain_db: L2DataStoreView,
) -> anyhow::Result<
    ServiceRunner<
        UninitializedTask<L2DataStoreView, GasPriceStore, Metadata, DA, SettingsProvider>,
    >,
>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData,
    SettingsProvider: GasPriceSettingsProvider,
    Metadata: MetadataStorage,
    DA: DaBlockCostsSource,
{
    let v1_config = config.v1().ok_or(anyhow::anyhow!("Expected V1 config"))?;
    let gas_price_init = UninitializedTask::new(
        v1_config,
        genesis_block_height,
        settings,
        block_stream,
        gas_price_db,
        metadata,
        da_source,
        on_chain_db,
    )?;
    Ok(ServiceRunner::new(gas_price_init))
}
