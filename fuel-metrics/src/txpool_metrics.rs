use prometheus_client::{
    registry::Registry,
};
use lazy_static::lazy_static;
use std::sync::RwLock;

pub use prometheus_client::metrics::counter::Counter;

#[derive(Default)]
pub struct TxPoolMetrics {
    pub tx_statistics: Registry
}

impl TxPoolMetrics {
    pub fn init(&mut self) {
        //metrics.tx_statistics.register(name, help, metric);
    }
}

lazy_static! {
    pub static ref TXPOOL_METRICS: RwLock<TxPoolMetrics> = Default::default();
}