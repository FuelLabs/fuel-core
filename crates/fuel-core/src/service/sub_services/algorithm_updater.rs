use crate::{
    database::{
        database_description::{
            gas_price::GasPriceDatabase,
            on_chain::OnChain,
        },
        Database,
        RegularStage,
    },
    service::{
        adapters::ConsensusParametersProvider,
        Config,
    },
};

use fuel_core_gas_price_service::fuel_gas_price_updater::{
    fuel_core_storage_adapter::{
        FuelL2BlockSource,
        GasPriceSettingsProvider,
    },
    AlgorithmUpdater,
    AlgorithmUpdaterV0,
    FuelGasPriceUpdater,
    MetadataStorage,
    UpdaterMetadata,
    V0Metadata,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    structured_storage::StructuredStorage,
    tables::{
        FuelBlocks,
        Transactions,
    },
    transactional::HistoricalView,
    StorageAsRef,
};
use fuel_core_types::{
    fuel_tx::field::MintAmount,
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use std::cmp::Ordering;

pub fn get_synced_gas_price_updater(
    config: &Config,
    genesis_block_height: BlockHeight,
    settings: ConsensusParametersProvider,
    block_stream: BoxStream<SharedImportResult>,
    mut gas_price_db: Database<GasPriceDatabase, RegularStage<GasPriceDatabase>>,
    on_chain_db: Database<OnChain, RegularStage<OnChain>>,
) -> anyhow::Result<
    FuelGasPriceUpdater<
        FuelL2BlockSource<ConsensusParametersProvider>,
        StructuredStorage<Database<GasPriceDatabase, RegularStage<GasPriceDatabase>>>,
    >,
> {
    let metadata_height: u32 = gas_price_db
        .latest_height()?
        .unwrap_or(genesis_block_height)
        .into();
    let latest_block_height: u32 = on_chain_db
        .latest_height()?
        .unwrap_or(genesis_block_height)
        .into();

    let genesis_metadata = UpdaterMetadata::V0(V0Metadata {
        new_exec_price: config.starting_gas_price,
        min_exec_gas_price: config.min_gas_price,
        exec_gas_price_change_percent: config.gas_price_change_percent,
        l2_block_height: genesis_block_height.into(),
        l2_block_fullness_threshold_percent: config.gas_price_threshold_percent,
    });
    if BlockHeight::from(latest_block_height) == genesis_block_height {
        let metadata_storage = StructuredStorage::new(gas_price_db);
        let l2_block_source =
            FuelL2BlockSource::new(genesis_block_height, settings, block_stream);
        let updater = FuelGasPriceUpdater::new(
            genesis_metadata.into(),
            l2_block_source,
            metadata_storage,
        );
        Ok(updater)
    } else {
        let metadata_storage = match metadata_height.cmp(&latest_block_height) {
            Ordering::Equal => StructuredStorage::new(gas_price_db),
            Ordering::Less => {
                let mut metadata_storage = StructuredStorage::new(gas_price_db);
                sync_metadata_storage_with_on_chain_storage(
                    &settings,
                    &mut metadata_storage,
                    on_chain_db,
                    metadata_height,
                    latest_block_height,
                    genesis_metadata,
                    genesis_block_height.into(),
                )?;
                metadata_storage
            }
            Ordering::Greater => {
                revert_gas_price_db_to_height(
                    &mut gas_price_db,
                    latest_block_height.into(),
                )?;
                StructuredStorage::new(gas_price_db)
            }
        };

        let l2_block_source =
            FuelL2BlockSource::new(genesis_block_height, settings, block_stream);

        FuelGasPriceUpdater::init(
            latest_block_height.into(),
            l2_block_source,
            metadata_storage,
        )
        .map_err(|e| anyhow::anyhow!("Could not initialize gas price updater: {e:?}"))
    }
}

fn sync_metadata_storage_with_on_chain_storage(
    settings: &ConsensusParametersProvider,
    metadata_storage: &mut StructuredStorage<
        Database<GasPriceDatabase, RegularStage<GasPriceDatabase>>,
    >,
    on_chain_db: Database<OnChain, RegularStage<OnChain>>,
    metadata_height: u32,
    latest_block_height: u32,
    default_metadata: UpdaterMetadata,
    genesis_block_height: u32,
) -> anyhow::Result<()> {
    let metadata = if metadata_height == genesis_block_height {
        default_metadata
    } else {
        metadata_storage
            .get_metadata(&metadata_height.into())?
            .ok_or(anyhow::anyhow!(
                "Expected metadata to exist for height: {metadata_height}"
            ))?
    };
    let mut inner: AlgorithmUpdater = metadata.into();
    match &mut inner {
        AlgorithmUpdater::V0(ref mut updater) => {
            sync_v0_metadata(
                settings,
                on_chain_db,
                metadata_height,
                latest_block_height,
                updater,
                metadata_storage,
            )?;
        }
    }
    Ok(())
}

fn sync_v0_metadata(
    settings: &ConsensusParametersProvider,
    on_chain_db: Database<OnChain, RegularStage<OnChain>>,
    metadata_height: u32,
    latest_block_height: u32,
    updater: &mut AlgorithmUpdaterV0,
    metadata_storage: &mut StructuredStorage<
        Database<GasPriceDatabase, RegularStage<GasPriceDatabase>>,
    >,
) -> anyhow::Result<()> {
    for height in (metadata_height + 1)..=latest_block_height {
        let view = on_chain_db.view_at(&height.into())?;
        let block = view
            .storage::<FuelBlocks>()
            .get(&height.into())?
            .ok_or(anyhow::anyhow!("Expected block to exist"))?;
        let last_tx_id = block.transactions().last().ok_or(anyhow::anyhow!(
            "Expected block to have at least one transaction"
        ))?;
        let param_version = block.header().consensus_parameters_version;
        let params = settings.settings(&param_version)?;
        let mint = view
            .storage::<Transactions>()
            .get(&last_tx_id)?
            .ok_or(anyhow::anyhow!("Expected tx to exist for id: {last_tx_id}"))?
            .as_mint()
            .ok_or(anyhow::anyhow!("Expected tx to be a mint"))?
            .to_owned();
        let block_gas_used = mint.mint_amount();
        let block_gas_capacity = params.block_gas_limit.try_into()?;
        updater.update_l2_block_data(height, *block_gas_used, block_gas_capacity)?;
        let metadata = AlgorithmUpdater::V0(updater.clone()).into();
        metadata_storage.set_metadata(metadata)?;
    }
    Ok(())
}

fn revert_gas_price_db_to_height(
    gas_price_db: &mut Database<GasPriceDatabase, RegularStage<GasPriceDatabase>>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    if let Some(gas_price_db_height) = gas_price_db.latest_height()? {
        let gas_price_db_height: u32 = gas_price_db_height.into();
        let height: u32 = height.into();
        let diff = gas_price_db_height - height;
        for _ in 0..diff {
            gas_price_db.rollback_last_block()?;
        }
    }
    Ok(())
}
