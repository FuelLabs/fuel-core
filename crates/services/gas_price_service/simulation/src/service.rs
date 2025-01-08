use crate::data_sources::{
    SimulatedDACosts,
    SimulatedL2Blocks,
};
use fuel_core_gas_price_service::{
    common::{
        fuel_core_storage_adapter::storage::GasPriceColumn,
        utils::BlockInfo,
    },
    v1::{
        algorithm::SharedV1Algorithm,
        da_source_service::{
            service::{
                new_da_service,
                DaSourceService,
            },
            DaBlockCosts,
        },
        metadata::{
            v1_algorithm_from_metadata,
            V1AlgorithmConfig,
            V1Metadata,
        },
        service::{
            GasPriceServiceV1,
            LatestGasPrice,
        },
    },
};
use fuel_core_services::{
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_storage::{
    structured_storage::test::InMemoryStorage,
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use fuel_gas_price_algorithm::v1::AlgorithmUpdaterV1;
use std::{
    num::NonZero,
    sync::{
        Arc,
        Mutex,
    },
};

fn read_config_from_file(_config_path: &str) -> V1AlgorithmConfig {
    // TODO: read from file and/or CLI
    V1AlgorithmConfig {
        new_exec_gas_price: 0,
        min_exec_gas_price: 0,
        exec_gas_price_change_percent: 0,
        l2_block_fullness_threshold_percent: 0,
        gas_price_factor: NonZero::new(100).unwrap(),
        min_da_gas_price: 0,
        max_da_gas_price_change_percent: 0,
        da_p_component: 0,
        da_d_component: 0,
        normal_range_size: 0,
        capped_range_size: 0,
        decrease_range_size: 0,
        block_activity_threshold: 0,
        da_poll_interval: None,
    }
}

fn read_metadata_from_file(_metadata_path: &str) -> V1Metadata {
    // TODO: read from file and/or CLI
    V1Metadata {
        new_scaled_exec_price: 0,
        l2_block_height: 0,
        new_scaled_da_gas_price: 0,
        gas_price_factor: NonZero::new(100).unwrap(),
        total_da_rewards_excess: 0,
        latest_known_total_da_cost_excess: 0,
        last_profit: 0,
        second_to_last_profit: 0,
        latest_da_cost_per_byte: 0,
        unrecorded_block_bytes: 0,
    }
}

fn get_updater() -> AlgorithmUpdaterV1 {
    let metadata_path = "TODO";
    let metadata = read_metadata_from_file(metadata_path);
    let config_path = "TODO";
    let config = read_config_from_file(config_path);
    v1_algorithm_from_metadata(metadata, &config)
}
type GasPriceStorage = StorageTransaction<InMemoryStorage<GasPriceColumn>>;

fn database() -> GasPriceStorage {
    InMemoryStorage::default().into_transaction()
}

pub struct ServiceController {
    service: GasPriceServiceV1<SimulatedL2Blocks, SimulatedDACosts, GasPriceStorage>,
    l2_block_sender: tokio::sync::mpsc::Sender<BlockInfo>,
    da_costs_sender: tokio::sync::mpsc::Sender<DaBlockCosts>,
}

impl ServiceController {
    pub async fn advance(
        &mut self,
        l2_block: BlockInfo,
        da_costs: Option<DaBlockCosts>,
    ) -> anyhow::Result<()> {
        if let Some(da_costs) = da_costs {
            self.da_costs_sender.send(da_costs).await?;
            self.run().await?;
        }
        self.l2_block_sender.send(l2_block).await?;
        self.run().await?;
        Ok(())
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let mut watcher = StateWatcher::started();
        match self.service.run(&mut watcher).await {
            TaskNextAction::ErrorContinue(err) => Err(err),
            _ => Ok(()),
        }
    }
}

fn poll_interval() -> Option<std::time::Duration> {
    Some(std::time::Duration::from_millis(1))
}

// this is async to mark that this must be run with a runtime although not specified by the type system :barf:
// (due to the `interval` function used internally)
pub async fn get_service_controller() -> ServiceController {
    let algorithm_updater = get_updater();
    let algo = algorithm_updater.algorithm();
    let shared_algo = SharedV1Algorithm::new_with_algorithm(algo);

    let (l2_block_source, l2_block_sender) = SimulatedL2Blocks::new_with_sender();
    let (da_block_source, da_costs_sender) = SimulatedDACosts::new_with_sender();

    let poll_interval = poll_interval();
    let latest_l2_height = Arc::new(Mutex::new(BlockHeight::new(0)));
    let latest_gas_price = LatestGasPrice::new(0, 0);
    let da_source_service =
        new_da_service(da_block_source, poll_interval, latest_l2_height.clone());
    let db = database();
    let service = GasPriceServiceV1::new(
        l2_block_source,
        shared_algo,
        latest_gas_price,
        algorithm_updater,
        da_source_service,
        db,
        latest_l2_height,
    );
    ServiceController {
        service,
        l2_block_sender,
        da_costs_sender,
    }
}
