use crate::global_registry;
use prometheus_client::metrics::gauge::Gauge;
use std::sync::OnceLock;

#[derive(Debug)]
pub struct GasPriceMetrics {
    pub real_gas_price: Gauge,
    pub exec_gas_price: Gauge,
    pub da_gas_price: Gauge,
    pub total_reward: Gauge,
    pub total_known_costs: Gauge,
    pub predicted_profit: Gauge,
    pub unrecorded_bytes: Gauge,
    pub latest_cost_per_byte: Gauge,
    pub recorded_height: Gauge,
}

impl Default for GasPriceMetrics {
    fn default() -> Self {
        let real_gas_price = Gauge::default();
        let exec_gas_price = Gauge::default();
        let da_gas_price = Gauge::default();
        let total_reward = Gauge::default();
        let total_known_costs = Gauge::default();
        let predicted_profit = Gauge::default();
        let unrecorded_bytes = Gauge::default();
        let latest_cost_per_byte = Gauge::default();
        let recorded_height = Gauge::default();

        let metrics = GasPriceMetrics {
            real_gas_price,
            exec_gas_price,
            da_gas_price,
            total_reward,
            total_known_costs,
            predicted_profit,
            unrecorded_bytes,
            latest_cost_per_byte,
            recorded_height,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "gas_price_service_real_gas_price",
            "The real gas price used on the most recent block",
            metrics.real_gas_price.clone(),
        );
        registry.register(
            "gas_price_service_exec_gas_price",
            "The requested execution gas price for the next block",
            metrics.exec_gas_price.clone(),
        );
        registry.register(
            "gas_price_service_da_gas_price",
            "The requested data availability gas price for the next block",
            metrics.da_gas_price.clone(),
        );
        registry.register(
            "gas_price_service_total_reward",
            "The total reward received from DA gas price fees",
            metrics.total_reward.clone(),
        );
        registry.register(
            "gas_price_service_total_known_costs",
            "The total known costs for committing L2 blocks to DA",
            metrics.total_known_costs.clone(),
        );
        registry.register(
            "gas_price_service_predicted_profit",
            "The predicted profit based on the rewards, known costs, and predicted costs from price per byte",
            metrics.predicted_profit.clone(),
        );
        registry.register(
            "gas_price_service_unrecorded_bytes",
            "The total bytes of all L2 blocks waiting to be recorded on DA",
            metrics.unrecorded_bytes.clone(),
        );
        registry.register(
            "gas_price_service_latest_cost_per_byte",
            "The latest cost per byte to record L2 blocks on DA",
            metrics.latest_cost_per_byte.clone(),
        );

        registry.register(
            "gas_price_service_recorded_height",
            "The height of the latest L2 block recorded on DA",
            metrics.recorded_height.clone(),
        );

        metrics
    }
}

static GAS_PRICE_METRICS: OnceLock<GasPriceMetrics> = OnceLock::new();

pub fn gas_price_metrics() -> &'static GasPriceMetrics {
    GAS_PRICE_METRICS.get_or_init(GasPriceMetrics::default)
}
