use crate::data_sources::{
    SimulatedDACosts,
    SimulatedL2Blocks,
};
use fuel_core_gas_price_service::{
    common::{
        fuel_core_storage_adapter::storage::GasPriceColumn,
        utils::BlockInfo,
    },
    ports::GetMetadataStorage,
    v1::{
        algorithm::SharedV1Algorithm,
        da_source_service::{
            service::new_da_service,
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
    Service,
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
        min_da_gas_price: 1_000,
        max_da_gas_price: u64::MAX,
        max_da_gas_price_change_percent: 10,
        da_p_component: 50_000_000__000_000_000,
        da_d_component: 100_000__000_000_000,
        normal_range_size: 0,
        capped_range_size: 0,
        decrease_range_size: 0,
        block_activity_threshold: 0,
        da_poll_interval: None,
    }
}

fn read_metadata_from_file(_metadata_path: &str, starting_height: u32) -> V1Metadata {
    // TODO: read from file and/or CLI
    let l2_block_height = starting_height - 1;
    let gas_price_factor = 100;
    // let new_scaled_da_gas_price = 3_547_063 * gas_price_factor / 1_000;
    // let new_scaled_da_gas_price = 15_893_241 * gas_price_factor / 1_000;
    let new_scaled_da_gas_price = 18_963 * gas_price_factor;
    V1Metadata {
        new_scaled_exec_price: 0,
        l2_block_height,
        new_scaled_da_gas_price,
        gas_price_factor: NonZero::new(gas_price_factor).unwrap(),
        total_da_rewards_excess: 0,
        latest_known_total_da_cost_excess: 0,
        last_profit: 0,
        second_to_last_profit: 0,
        latest_da_cost_per_byte: 625504961,
        unrecorded_block_bytes: 0,
    }
}

fn get_updater(starting_height: u32) -> AlgorithmUpdaterV1 {
    let metadata_path = "TODO";
    let metadata = read_metadata_from_file(metadata_path, starting_height);
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
        da_costs: Vec<DaBlockCosts>,
    ) -> anyhow::Result<()> {
        tracing::debug!(
            "advancing service with l2_block: {:?} and da_costs: {:?}",
            l2_block,
            da_costs
        );
        for costs in da_costs {
            tracing::debug!("sending da_costs: {:?}", costs);
            self.da_costs_sender.send(costs).await?;
            tracing::debug!("sent da_costs");
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

    pub fn gas_price(&self) -> u64 {
        self.service.next_block_algorithm().next_gas_price()
    }

    pub fn profit_cost_reward(&self) -> anyhow::Result<(i128, u128, u128)> {
        let latest_height = self.service.latest_l2_block();
        let metadata = self
            .service
            .storage_tx_provider()
            .get_metadata(&latest_height)?
            .ok_or(anyhow::anyhow!("no metadata found"))?;
        if let Some(values) = metadata.v1().map(|m| {
            (
                m.last_profit,
                m.latest_known_total_da_cost_excess,
                m.total_da_rewards_excess,
            )
        }) {
            Ok(values)
        } else {
            Err(anyhow::anyhow!("no profit found"))
        }
    }
}

fn poll_interval() -> Option<std::time::Duration> {
    Some(std::time::Duration::from_millis(1))
}

pub async fn get_service_controller(
    starting_height: u32,
) -> anyhow::Result<ServiceController> {
    tracing::info!("creating service controller");
    let algorithm_updater = get_updater(starting_height);
    let algo = algorithm_updater.algorithm();
    let shared_algo = SharedV1Algorithm::new_with_algorithm(algo);

    let (l2_block_source, l2_block_sender) =
        SimulatedL2Blocks::new_with_sender(shared_algo.clone());
    let (da_block_source, da_costs_sender) = SimulatedDACosts::new_with_sender();

    let poll_interval = poll_interval();
    let latest_l2_height = Arc::new(Mutex::new(BlockHeight::new(0)));
    let latest_gas_price = LatestGasPrice::new(0, 0);
    let da_source_service =
        new_da_service(da_block_source, poll_interval, latest_l2_height.clone());
    da_source_service.start_and_await().await?;
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
    let controller = ServiceController {
        service,
        l2_block_sender,
        da_costs_sender,
    };
    Ok(controller)
}
