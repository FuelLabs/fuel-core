use crate::{
    common::utils::Error,
    v0::metadata::V0Metadata,
    v1::metadata::V1Metadata,
};
use fuel_core_types::fuel_types::BlockHeight;
use fuel_gas_price_algorithm::{
    v0::AlgorithmUpdaterV0,
    v1::AlgorithmUpdaterV1,
};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum UpdaterMetadata {
    V0(V0Metadata),
    V1(V1Metadata),
}

impl UpdaterMetadata {
    pub fn l2_block_height(&self) -> BlockHeight {
        match self {
            UpdaterMetadata::V0(v0) => v0.l2_block_height.into(),
            UpdaterMetadata::V1(v1) => v1.l2_block_height.into(),
        }
    }
}

impl From<AlgorithmUpdaterV0> for UpdaterMetadata {
    fn from(updater: AlgorithmUpdaterV0) -> Self {
        Self::V0(updater.into())
    }
}

impl From<AlgorithmUpdaterV1> for UpdaterMetadata {
    fn from(updater: AlgorithmUpdaterV1) -> Self {
        Self::V1(updater.into())
    }
}

impl TryFrom<UpdaterMetadata> for AlgorithmUpdaterV0 {
    type Error = Error;

    fn try_from(metadata: UpdaterMetadata) -> Result<Self, Self::Error> {
        match metadata {
            UpdaterMetadata::V0(v0) => Ok(AlgorithmUpdaterV0::from(v0)),
            _ => Err(Error::CouldNotConvertMetadata),
        }
    }
}

impl TryFrom<UpdaterMetadata> for AlgorithmUpdaterV1 {
    type Error = Error;

    fn try_from(metadata: UpdaterMetadata) -> Result<Self, Self::Error> {
        match metadata {
            UpdaterMetadata::V1(v1) => Ok(AlgorithmUpdaterV1::from(v1)),
            _ => Err(Error::CouldNotConvertMetadata),
        }
    }
}

impl TryFrom<UpdaterMetadata> for V0Metadata {
    type Error = Error;

    fn try_from(metadata: UpdaterMetadata) -> Result<Self, Self::Error> {
        match metadata {
            UpdaterMetadata::V0(v0) => Ok(v0),
            _ => Err(Error::CouldNotConvertMetadata),
        }
    }
}

impl TryFrom<UpdaterMetadata> for V1Metadata {
    type Error = Error;

    fn try_from(metadata: UpdaterMetadata) -> Result<Self, Self::Error> {
        match metadata {
            UpdaterMetadata::V1(v1) => Ok(v1),
            _ => Err(Error::CouldNotConvertMetadata),
        }
    }
}
