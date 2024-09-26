use crate::{
    common::{
        fuel_core_storage_adapter::{
            get_block_info,
            storage::GasPriceColumn,
            GasPriceSettings,
            GasPriceSettingsProvider,
        },
        gas_price_algorithm::SharedGasPriceAlgo,
        l2_block_source::FuelL2BlockSource,
        updater_metadata::UpdaterMetadata,
        utils::BlockInfo,
    },
    ports::{
        GasPriceData,
        GasPriceServiceConfig,
        L2Data,
        MetadataStorage,
    },
    v0::{
        metadata::V0Metadata,
        service::GasPriceServiceV0,
    },
};
use fuel_core_services::{
    stream::BoxStream,
    ServiceRunner,
};
use fuel_core_storage::{
    kv_store::KeyValueInspect,
    not_found,
    structured_storage::StructuredStorage,
    transactional::{
        AtomicView,
        Modifiable,
    },
};
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use fuel_gas_price_algorithm::v0::AlgorithmUpdaterV0;

pub use fuel_gas_price_algorithm::v0::AlgorithmV0;

pub type SharedV0Algorithm = SharedGasPriceAlgo<AlgorithmV0>;

struct UninitializedTask<L2DataStoreView, GasPriceStore, SettingsProvider> {
    pub config: GasPriceServiceConfig,
    pub genesis_block_height: BlockHeight,
    pub settings: SettingsProvider,
    pub gas_price_db: GasPriceStore,
    pub on_chain_db: L2DataStoreView,
    pub block_stream: BoxStream<SharedImportResult>,
}

fn get_default_metadata(
    config: &GasPriceServiceConfig,
    latest_block_height: u32,
) -> V0Metadata {
    V0Metadata {
        new_exec_price: config.starting_gas_price.max(config.min_gas_price),
        min_exec_gas_price: config.min_gas_price,
        exec_gas_price_change_percent: config.gas_price_change_percent,
        l2_block_height: latest_block_height,
        l2_block_fullness_threshold_percent: config.gas_price_threshold_percent,
    }
}

impl<L2DataStore, L2DataStoreView, GasPriceStore, SettingsProvider>
    UninitializedTask<L2DataStoreView, GasPriceStore, SettingsProvider>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData + Modifiable + KeyValueInspect<Column = GasPriceColumn>,
    SettingsProvider: GasPriceSettingsProvider,
{
    pub fn new(
        config: GasPriceServiceConfig,
        genesis_block_height: BlockHeight,
        settings: SettingsProvider,
        block_stream: BoxStream<SharedImportResult>,
        gas_price_db: GasPriceStore,
        on_chain_db: L2DataStoreView,
    ) -> anyhow::Result<Self> {
        let task = Self {
            config,
            genesis_block_height,
            settings,
            gas_price_db,
            on_chain_db,
            block_stream,
        };
        Ok(task)
    }

    pub fn init(
        self,
    ) -> anyhow::Result<
        GasPriceServiceV0<
            FuelL2BlockSource<SettingsProvider>,
            StructuredStorage<GasPriceStore>,
        >,
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

        let mut metadata_storage = StructuredStorage::new(self.gas_price_db);

        if BlockHeight::from(latest_block_height) == self.genesis_block_height
            || first_run
        {
            let service = GasPriceServiceV0::new(
                l2_block_source,
                metadata_storage,
                get_default_metadata(&self.config, latest_block_height),
            )?;
            Ok(service)
        } else {
            if latest_block_height > metadata_height {
                sync_gas_price_db_with_on_chain_storage(
                    &self.settings,
                    &mut metadata_storage,
                    &self.on_chain_db,
                    metadata_height,
                    latest_block_height,
                )?;
            }

            let service = GasPriceServiceV0::new(
                l2_block_source,
                metadata_storage,
                get_default_metadata(&self.config, latest_block_height),
            )?;
            Ok(service)
        }
    }
}

fn sync_gas_price_db_with_on_chain_storage<
    L2DataStore,
    L2DataStoreView,
    GasPriceStore,
    SettingsProvider,
>(
    settings: &SettingsProvider,
    metadata_storage: &mut StructuredStorage<GasPriceStore>,
    on_chain_db: &L2DataStoreView,
    metadata_height: u32,
    latest_block_height: u32,
) -> anyhow::Result<()>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData + Modifiable + KeyValueInspect<Column = GasPriceColumn>,
    SettingsProvider: GasPriceSettingsProvider,
{
    let UpdaterMetadata::V0(metadata) = metadata_storage
        .get_metadata(&metadata_height.into())?
        .ok_or(anyhow::anyhow!(
            "Expected metadata to exist for height: {metadata_height}"
        ))?;

    let mut algo_updater = metadata.into();

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

fn sync_v0_metadata<L2DataStore, L2DataStoreView, GasPriceStore, SettingsProvider>(
    settings: &SettingsProvider,
    on_chain_db: &L2DataStoreView,
    metadata_height: u32,
    latest_block_height: u32,
    updater: &mut AlgorithmUpdaterV0,
    metadata_storage: &mut StructuredStorage<GasPriceStore>,
) -> anyhow::Result<()>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData + Modifiable + KeyValueInspect<Column = GasPriceColumn>,
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

pub fn new_gas_price_service_v0<
    L2DataStore,
    L2DataStoreView,
    GasPriceStore,
    SettingsProvider,
>(
    config: GasPriceServiceConfig,
    genesis_block_height: BlockHeight,
    settings: SettingsProvider,
    block_stream: BoxStream<SharedImportResult>,
    gas_price_db: GasPriceStore,
    on_chain_db: L2DataStoreView,
) -> anyhow::Result<
    ServiceRunner<
        GasPriceServiceV0<
            FuelL2BlockSource<SettingsProvider>,
            StructuredStorage<GasPriceStore>,
        >,
    >,
>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData + Modifiable + KeyValueInspect<Column = GasPriceColumn>,
    SettingsProvider: GasPriceSettingsProvider,
{
    let gas_price_init = UninitializedTask::new(
        config,
        genesis_block_height,
        settings,
        block_stream,
        gas_price_db,
        on_chain_db,
    )?;
    let gas_price_service_v0 = gas_price_init.init()?;
    Ok(ServiceRunner::new(gas_price_service_v0))
}
