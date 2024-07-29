use crate::{
    serialization::HexIfHumanReadable,
    TableEntry,
};
use fuel_core_types::{
    fuel_tx::{
        AssetId,
        BlobId,
        StorageSlot,
    },
    fuel_types::Bytes32,
    fuel_vm::{
        BlobBytes,
        BlobData,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::serde_as;

#[serde_as]
#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct BlobConfig {
    pub blob_id: BlobId,
    #[serde_as(as = "HexIfHumanReadable")]
    pub payload: Vec<u8>,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, PartialOrd, Ord)]
pub struct BlobStateConfig {
    pub key: Bytes32,
    #[serde_as(as = "HexIfHumanReadable")]
    pub value: Vec<u8>,
}

impl TryFrom<BlobStateConfig> for StorageSlot {
    type Error = anyhow::Error;

    fn try_from(value: BlobStateConfig) -> Result<Self, Self::Error> {
        let key = value.key;
        let value = Bytes32::try_from(value.value.as_slice())?;
        Ok(Self::new(key, value))
    }
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for BlobStateConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            key: crate::Randomize::randomize(&mut rng),
            value: Bytes32::randomize(&mut rng).to_vec(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct BlobBalanceConfig {
    pub asset_id: AssetId,
    pub amount: u64,
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for BlobBalanceConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            asset_id: crate::Randomize::randomize(&mut rng),
            amount: crate::Randomize::randomize(&mut rng),
        }
    }
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for BlobConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        use fuel_core_types::fuel_tx::BlobIdExt;

        let payload_len = rng.gen_range(32..128);
        let mut payload = vec![0; payload_len as usize];
        rng.fill_bytes(&mut payload);

        let blob_id = BlobId::compute(&payload);

        Self { blob_id, payload }
    }
}

impl From<TableEntry<BlobData>> for BlobConfig {
    fn from(value: TableEntry<BlobData>) -> Self {
        BlobConfig {
            blob_id: value.key,
            payload: value.value.0.to_vec(),
        }
    }
}

impl From<BlobConfig> for TableEntry<BlobData> {
    fn from(config: BlobConfig) -> Self {
        Self {
            key: config.blob_id,
            value: BlobBytes(config.payload),
        }
    }
}
