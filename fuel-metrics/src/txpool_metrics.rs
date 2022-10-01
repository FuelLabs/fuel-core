use lazy_static::lazy_static;
use prometheus::{
    self,
    register_int_gauge,
    IntGauge,
    Histogram, register_histogram
};

/// TxPoolMetrics is a wrapper struct for all
/// of the initialized counters for TxPool-related metrics
#[derive(Clone, Debug)]
pub struct TxPoolMetrics {
    pub average_gas_price_gauge: IntGauge,
    pub median_gas_price_hist: Histogram,
}

lazy_static! {
    pub static ref TXPOOL_METRICS: TxPoolMetrics = TxPoolMetrics {
        average_gas_price_gauge: register_int_gauge!(
            "Average_Gas_Price",
            "The Average Gase Price of Transactions in the TxPool"
        )
        .unwrap(),
        median_gas_price_hist: register_histogram!(
            "Median_Gas_Price",
            "The Median Gase Price of Transactions in the TxPool"
        )
        .unwrap()
    };
}