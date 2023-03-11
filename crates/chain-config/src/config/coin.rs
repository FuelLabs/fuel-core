use crate::{
    serialization::{
        HexNumber,
        HexType,
    },
    GenesisCommitment,
};
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    entities::coin::CompressedCoin,
    fuel_crypto::Hasher,
    fuel_types::{
        Address,
        AssetId,
        Bytes32,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
};

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct CoinConfig {
    /// auto-generated if None
    #[serde_as(as = "Option<HexType>")]
    #[serde(default)]
    pub tx_id: Option<Bytes32>,
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub output_index: Option<u8>,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub tx_pointer_block_height: Option<BlockHeight>,
    /// used if coin is forked from another chain to preserve id & tx_pointer
    /// The index of the originating tx within `tx_pointer_block_height`
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub tx_pointer_tx_idx: Option<u16>,
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub maturity: Option<BlockHeight>,
    #[serde_as(as = "HexType")]
    pub owner: Address,
    #[serde_as(as = "HexNumber")]
    pub amount: u64,
    #[serde_as(as = "HexType")]
    pub asset_id: AssetId,
}

impl GenesisCommitment for CompressedCoin {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let coin_hash = *Hasher::default()
            .chain(self.owner)
            .chain(self.amount.to_be_bytes())
            .chain(self.asset_id)
            .chain((*self.maturity).to_be_bytes())
            .chain([self.status as u8])
            .chain(self.tx_pointer.block_height().to_be_bytes())
            .chain(self.tx_pointer.tx_index().to_be_bytes())
            .finalize();

        Ok(coin_hash)
    }
}
