use chrono::{DateTime, TimeZone, Utc};
use fuel_tx::crypto::Hasher;
use fuel_tx::Bytes32;
use serde::{Deserialize, Serialize};
use std::iter::FromIterator;

pub type BlockHeight = u32;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FuelBlock {
    pub fuel_height: BlockHeight,
    pub transactions: Vec<Bytes32>,
    pub time: DateTime<Utc>,
}

impl Default for FuelBlock {
    fn default() -> Self {
        Self {
            fuel_height: 0,
            transactions: vec![],
            time: Utc.timestamp(0, 0),
        }
    }
}

impl FuelBlock {
    pub fn id(&self) -> Bytes32 {
        let mut hasher = Hasher::from_iter(&self.transactions);
        hasher.input(self.fuel_height.to_be_bytes());
        hasher.input(self.time.timestamp_millis().to_be_bytes());
        hasher.digest()
    }
}
