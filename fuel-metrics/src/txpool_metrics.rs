use prometheus_client::{
    registry::Registry,
};
use lazy_static::lazy_static;
use std::sync::RwLock;
use std::boxed::Box;
use std::default::Default;

pub use prometheus_client::metrics::histogram::Histogram;

pub struct TxPoolMetrics {
    pub tx_statistics: RwLock<Registry<Box<Histogram>>>,
    pub gas_price_histogram: Box<Histogram>,
}

impl Default for TxPoolMetrics {
    fn default() -> Self {
        let gas_prices = Vec::new();
        let gas_price_histogram = Box::new(Histogram::new(gas_prices.into_iter()));

        let tx_statistics = RwLock::new(Registry::default());

        tx_statistics.write().unwrap().register("Tx_Gas_Price_Histogram", "A Histogram keeping track of all gas prices for each tx in the mempool", gas_price_histogram.clone());

        Self {
            tx_statistics,
            gas_price_histogram,
        }
    }
}

impl TxPoolMetrics {
    pub fn init(&mut self) {                
    }
}

lazy_static! {
    pub static ref TXPOOL_METRICS: RwLock<TxPoolMetrics> = Default::default();
}