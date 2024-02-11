use crate::GenesisCommitment;
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    entities::coins::coin::CompressedCoin,
    fuel_crypto::Hasher,
    fuel_tx::{
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        BlockHeight,
        Bytes32,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Default)]
// If any fields are added make sure to update the `NonSkippingSerialize` impl
pub struct CoinConfig {
    /// auto-generated if None
    pub tx_id: Option<Bytes32>,
    pub output_index: Option<u8>,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    pub tx_pointer_block_height: Option<BlockHeight>,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    /// The index of the originating tx within `tx_pointer_block_height`
    pub tx_pointer_tx_idx: Option<u16>,
    pub maturity: Option<BlockHeight>,
    pub owner: Address,
    pub amount: u64,
    pub asset_id: AssetId,
}

#[cfg(feature = "parquet")]
impl crate::serialization::NonSkippingSerialize for CoinConfig {
    fn non_skipping_serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("CoinConfig", 8)?;
        s.serialize_field("tx_id", &self.tx_id)?;
        s.serialize_field("output_index", &self.output_index)?;
        s.serialize_field("tx_pointer_block_height", &self.tx_pointer_block_height)?;
        s.serialize_field("tx_pointer_tx_idx", &self.tx_pointer_tx_idx)?;
        s.serialize_field("maturity", &self.maturity)?;
        s.serialize_field("owner", &self.owner)?;
        s.serialize_field("amount", &self.amount)?;
        s.serialize_field("asset_id", &self.asset_id)?;
        s.end()
    }
}

impl CoinConfig {
    pub fn utxo_id(&self) -> Option<UtxoId> {
        match (self.tx_id, self.output_index) {
            (Some(tx_id), Some(output_index)) => Some(UtxoId::new(tx_id, output_index)),
            _ => None,
        }
    }

    pub fn tx_pointer(&self) -> TxPointer {
        match (self.tx_pointer_block_height, self.tx_pointer_tx_idx) {
            (Some(block_height), Some(tx_idx)) => TxPointer::new(block_height, tx_idx),
            _ => TxPointer::default(),
        }
    }
}

#[cfg(all(test, feature = "random", feature = "std"))]
impl crate::Randomize for CoinConfig {
    fn randomize(mut rng: impl ::rand::Rng) -> Self {
        Self {
            tx_id: rng
                .gen::<bool>()
                .then(|| super::random_bytes_32(&mut rng).into()),
            output_index: rng.gen::<bool>().then(|| rng.gen()),
            tx_pointer_block_height: rng
                .gen::<bool>()
                .then(|| BlockHeight::new(rng.gen())),
            tx_pointer_tx_idx: rng.gen::<bool>().then(|| rng.gen()),
            maturity: rng.gen::<bool>().then(|| BlockHeight::new(rng.gen())),
            owner: Address::new(super::random_bytes_32(&mut rng)),
            amount: rng.gen(),
            asset_id: AssetId::new(super::random_bytes_32(rng)),
        }
    }
}

impl GenesisCommitment for CompressedCoin {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let owner = self.owner();
        let amount = self.amount();
        let asset_id = self.asset_id();
        let maturity = self.maturity();
        let tx_pointer = self.tx_pointer();

        let coin_hash = *Hasher::default()
            .chain(owner)
            .chain(amount.to_be_bytes())
            .chain(asset_id)
            .chain((*maturity).to_be_bytes())
            .chain(tx_pointer.block_height().to_be_bytes())
            .chain(tx_pointer.tx_index().to_be_bytes())
            .finalize();

        Ok(coin_hash)
    }
}
