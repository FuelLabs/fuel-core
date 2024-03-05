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

#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct CoinConfig {
    /// auto-generated if None
    pub tx_id: Option<Bytes32>,
    pub output_index: Option<u8>,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    pub tx_pointer_block_height: Option<BlockHeight>,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    /// The index of the originating tx within `tx_pointer_block_height`
    pub tx_pointer_tx_idx: Option<u16>,
    pub owner: Address,
    pub amount: u64,
    pub asset_id: AssetId,
}

impl CoinConfig {
    // TODO: Remove https://github.com/FuelLabs/fuel-core/issues/1668
    pub fn utxo_id(&self) -> Option<UtxoId> {
        match (self.tx_id, self.output_index) {
            (Some(tx_id), Some(output_index)) => Some(UtxoId::new(tx_id, output_index)),
            _ => None,
        }
    }

    // TODO: Remove https://github.com/FuelLabs/fuel-core/issues/1668
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
        let tx_pointer = self.tx_pointer();

        let coin_hash = *Hasher::default()
            .chain(owner)
            .chain(amount.to_be_bytes())
            .chain(asset_id)
            .chain(tx_pointer.block_height().to_be_bytes())
            .chain(tx_pointer.tx_index().to_be_bytes())
            .finalize();

        Ok(coin_hash)
    }
}
