use crate::v0::metadata::V0Metadata;
use fuel_core_types::fuel_types::BlockHeight;
use fuel_gas_price_algorithm::v0::AlgorithmUpdaterV0;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum UpdaterMetadata {
    V0(V0Metadata),
}

impl UpdaterMetadata {
    pub fn l2_block_height(&self) -> BlockHeight {
        match self {
            UpdaterMetadata::V0(v1) => v1.l2_block_height.into(),
        }
    }
}

impl From<AlgorithmUpdaterV0> for UpdaterMetadata {
    fn from(updater: AlgorithmUpdaterV0) -> Self {
        Self::V0(updater.into())
    }
}
