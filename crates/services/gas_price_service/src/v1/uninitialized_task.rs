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
        GasPriceServiceConfig,
        GetDaSequenceNumber,
        GetMetadataStorage,
        L2Data,
        SetDaSequenceNumber,
        SetMetadataStorage,
        TransactionableStorage,
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
        },
        uninitialized_task::fuel_storage_unrecorded_blocks::FuelStorageUnrecordedBlocks,
    },
};
use anyhow::Error;
use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    Service,
    ServiceRunner,
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
use std::time::Duration;

pub mod fuel_storage_unrecorded_blocks;

pub struct UninitializedTask<
    L2DataStoreView,
    GasPriceStore,
    DA,
    SettingsProvider,
    PersistedData,
> {
    pub config: V1AlgorithmConfig,
    pub genesis_block_height: BlockHeight,
    pub settings: SettingsProvider,
    pub gas_price_db: GasPriceStore,
    pub on_chain_db: L2DataStoreView,
    pub block_stream: BoxStream<SharedImportResult>,
    pub(crate) shared_algo: SharedV1Algorithm,
    pub(crate) algo_updater: AlgorithmUpdaterV1,
    pub(crate) da_source: DA,
    pub persisted_data: PersistedData,
}

impl<
        L2DataStore,
        L2DataStoreView,
        GasPriceStore,
        DA,
        SettingsProvider,
        PersistedData,
    >
    UninitializedTask<L2DataStoreView, GasPriceStore, DA, SettingsProvider, PersistedData>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData,
    PersistedData: GetMetadataStorage + GetDaSequenceNumber,
    PersistedData: TransactionableStorage,
    for<'a> PersistedData::Transaction<'a>:
        SetMetadataStorage + UnrecordedBlocks + SetDaSequenceNumber,
    DA: DaBlockCostsSource + 'static,
    SettingsProvider: GasPriceSettingsProvider,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: V1AlgorithmConfig,
        genesis_block_height: BlockHeight,
        settings: SettingsProvider,
        block_stream: BoxStream<SharedImportResult>,
        gas_price_db: GasPriceStore,
        da_source: DA,
        on_chain_db: L2DataStoreView,
        persisted_data: PersistedData,
    ) -> anyhow::Result<Self> {
        let latest_block_height: u32 = on_chain_db
            .latest_view()?
            .latest_height()
            .unwrap_or(genesis_block_height)
            .into();

        let (algo_updater, shared_algo) =
            initialize_algorithm(&config, latest_block_height, &persisted_data)?;

        let task = Self {
            config,
            genesis_block_height,
            settings,
            gas_price_db,
            on_chain_db,
            block_stream,
            algo_updater,
            shared_algo,
            da_source,
            persisted_data,
        };
        Ok(task)
    }

    pub async fn init(
        mut self,
    ) -> anyhow::Result<
        GasPriceServiceV1<FuelL2BlockSource<SettingsProvider>, DA, PersistedData>,
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

        if let Some(sequence_number) = self
            .persisted_data
            .get_sequence_number(&metadata_height.into())?
        {
            self.da_source.set_last_value(sequence_number).await?;
        }
        let poll_duration = self
            .config
            .da_poll_interval
            .map(|x| Duration::from_millis(x.into()));
        let da_service = DaSourceService::new(self.da_source, poll_duration);
        let da_service_runner = ServiceRunner::new(da_service);
        da_service_runner.start_and_await().await?;

        if BlockHeight::from(latest_block_height) == self.genesis_block_height
            || first_run
        {
            let service = GasPriceServiceV1::new(
                l2_block_source,
                self.shared_algo,
                self.algo_updater,
                da_service_runner,
                self.persisted_data,
            );
            Ok(service)
        } else {
            if latest_block_height > metadata_height {
                sync_gas_price_db_with_on_chain_storage(
                    &self.settings,
                    &self.config,
                    &self.on_chain_db,
                    metadata_height,
                    latest_block_height,
                    &mut self.persisted_data,
                )?;
            }

            let service = GasPriceServiceV1::new(
                l2_block_source,
                self.shared_algo,
                self.algo_updater,
                da_service_runner,
                self.persisted_data,
            );
            Ok(service)
        }
    }
}

#[async_trait::async_trait]
impl<
        L2DataStore,
        L2DataStoreView,
        GasPriceStore,
        DA,
        SettingsProvider,
        PersistedData,
    > RunnableService
    for UninitializedTask<
        L2DataStoreView,
        GasPriceStore,
        DA,
        SettingsProvider,
        PersistedData,
    >
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    GasPriceStore: GasPriceData,
    DA: DaBlockCostsSource + 'static,
    SettingsProvider: GasPriceSettingsProvider + 'static,
    PersistedData:
        GetMetadataStorage + GetDaSequenceNumber + TransactionableStorage + 'static,
    for<'a> <PersistedData as TransactionableStorage>::Transaction<'a>:
        SetMetadataStorage + UnrecordedBlocks + SetDaSequenceNumber,
{
    const NAME: &'static str = "GasPriceServiceV1";
    type SharedData = SharedV1Algorithm;
    type Task = GasPriceServiceV1<FuelL2BlockSource<SettingsProvider>, DA, PersistedData>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_algo.clone()
    }

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        UninitializedTask::init(self).await
    }
}

fn sync_gas_price_db_with_on_chain_storage<
    'a,
    L2DataStore,
    L2DataStoreView,
    SettingsProvider,
    PersistedData,
>(
    settings: &SettingsProvider,
    config: &V1AlgorithmConfig,
    on_chain_db: &L2DataStoreView,
    metadata_height: u32,
    latest_block_height: u32,
    persisted_data: &'a mut PersistedData,
) -> anyhow::Result<()>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    SettingsProvider: GasPriceSettingsProvider,
    PersistedData: GetMetadataStorage + TransactionableStorage + 'a,
    <PersistedData as TransactionableStorage>::Transaction<'a>:
        SetMetadataStorage + UnrecordedBlocks,
{
    let metadata = persisted_data
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
        persisted_data,
    )?;

    Ok(())
}

fn sync_v1_metadata<
    'a,
    L2DataStore,
    L2DataStoreView,
    SettingsProvider,
    StorageTxGenerator,
>(
    settings: &SettingsProvider,
    on_chain_db: &L2DataStoreView,
    metadata_height: u32,
    latest_block_height: u32,
    updater: &mut AlgorithmUpdaterV1,
    storage_tx_generator: &'a mut StorageTxGenerator,
) -> anyhow::Result<()>
where
    L2DataStore: L2Data,
    L2DataStoreView: AtomicView<LatestView = L2DataStore>,
    SettingsProvider: GasPriceSettingsProvider,
    StorageTxGenerator: TransactionableStorage + 'a,
    <StorageTxGenerator as TransactionableStorage>::Transaction<'a>:
        SetMetadataStorage + UnrecordedBlocks,
{
    let first = metadata_height.saturating_add(1);
    let view = on_chain_db.latest_view()?;
    let mut tx = storage_tx_generator.begin_transaction()?;
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
            &mut tx,
        )?;
        let metadata: UpdaterMetadata = updater.clone().into();
        tx.set_metadata(&metadata)?;
    }
    StorageTxGenerator::commit_transaction(tx)?;

    Ok(())
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn new_gas_price_service_v1<
    L2DataStore,
    GasPriceStore,
    DA,
    SettingsProvider,
    PersistedData,
>(
    config: GasPriceServiceConfig,
    genesis_block_height: BlockHeight,
    settings: SettingsProvider,
    block_stream: BoxStream<SharedImportResult>,
    gas_price_db: GasPriceStore,
    da_source: DA,
    on_chain_db: L2DataStore,
    persisted_data: PersistedData,
) -> anyhow::Result<
    ServiceRunner<
        UninitializedTask<
            L2DataStore,
            GasPriceStore,
            DA,
            SettingsProvider,
            PersistedData,
        >,
    >,
>
where
    L2DataStore: AtomicView,
    L2DataStore::LatestView: L2Data,
    GasPriceStore: GasPriceData,
    SettingsProvider: GasPriceSettingsProvider,
    DA: DaBlockCostsSource,
    PersistedData:
        GetMetadataStorage + GetDaSequenceNumber + TransactionableStorage + 'static,
    for<'a> PersistedData::Transaction<'a>:
        SetMetadataStorage + UnrecordedBlocks + SetDaSequenceNumber,
{
    let v1_config = config.v1().ok_or(anyhow::anyhow!("Expected V1 config"))?;
    let gas_price_init = UninitializedTask::new(
        v1_config,
        genesis_block_height,
        settings,
        block_stream,
        gas_price_db,
        da_source,
        on_chain_db,
        persisted_data,
    )?;
    Ok(ServiceRunner::new(gas_price_init))
}
