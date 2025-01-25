use std::sync::OnceLock;
use prometheus_client::metrics::gauge::Gauge;


#[derive(Debug, Default)]
pub struct GasPriceMetrics {
    pub real_gas_price: Gauge,
    pub exec_gas_price: Gauge,
    pub da_gas_price: Gauge,
    pub total_reward: Gauge,
    pub total_known_costs: Gauge,
    pub predicted_profit: Gauge,
    pub unrecorded_bytes: Gauge,
    pub latest_cost_per_byte: Gauge,
}

static GAS_PRICE_METRICS: OnceLock<GasPriceMetrics> = OnceLock::new();

pub fn gas_price_metrics() -> &'static GasPriceMetrics {
    GAS_PRICE_METRICS.get_or_init(GasPriceMetrics::default)
}