use fuel_core_types::{
    entities::coins::coin::{
        CompressedCoin,
        CompressedCoinV1,
    },
    fuel_tx::{
        Address,
        AssetId,
        TxId,
        UtxoId,
        Word,
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
    },
};

#[derive(Debug, Eq)]
pub(crate) struct CoinInBatch {
    /// The utxo id
    pub utxo_id: UtxoId,
    /// The index of the transaction using this coin in the batch
    pub idx: usize,
    /// The TxId that use this coin (useful to remove them from the batch in case of skipped tx)
    pub tx_id: TxId,
    /// the owner of the coin
    pub owner: Address,
    /// the amount stored in the coin
    pub amount: Word,
    /// the asset the coin stores
    pub asset_id: AssetId,
}

impl PartialEq for CoinInBatch {
    fn eq(&self, other: &Self) -> bool {
        self.utxo() == other.utxo()
            && self.owner() == other.owner()
            && self.amount() == other.amount()
            && self.asset_id() == other.asset_id()
        // we don't include the idx here
    }
}

impl CoinInBatch {
    pub(crate) fn utxo(&self) -> &UtxoId {
        &self.utxo_id
    }

    pub(crate) fn idx(&self) -> usize {
        self.idx
    }

    pub(crate) fn owner(&self) -> &Address {
        &self.owner
    }

    pub(crate) fn amount(&self) -> &Word {
        &self.amount
    }

    pub(crate) fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }

    pub(crate) fn from_signed_coin(
        signed_coin: &CoinSigned,
        idx: usize,
        tx_id: TxId,
    ) -> Self {
        let CoinSigned {
            utxo_id,
            owner,
            amount,
            asset_id,
            ..
        } = signed_coin;

        CoinInBatch {
            utxo_id: *utxo_id,
            idx,
            tx_id,
            owner: *owner,
            amount: *amount,
            asset_id: *asset_id,
        }
    }

    pub(crate) fn from_predicate_coin(
        predicate_coin: &CoinPredicate,
        idx: usize,
        tx_id: TxId,
    ) -> Self {
        let CoinPredicate {
            utxo_id,
            owner,
            amount,
            asset_id,
            ..
        } = predicate_coin;

        CoinInBatch {
            utxo_id: *utxo_id,
            idx,
            tx_id,
            owner: *owner,
            amount: *amount,
            asset_id: *asset_id,
        }
    }

    pub(crate) fn equal_compressed_coin(&self, compressed_coin: &CompressedCoin) -> bool {
        match compressed_coin {
            CompressedCoin::V1(coin) => {
                self.owner() == &coin.owner
                    && self.amount() == &coin.amount
                    && self.asset_id() == &coin.asset_id
            }
            _ => {
                panic!("Unsupported compressed coin version");
            }
        }
    }
}

impl From<CoinInBatch> for CompressedCoin {
    fn from(value: CoinInBatch) -> Self {
        let CoinInBatch {
            owner,
            amount,
            asset_id,
            ..
        } = value;

        CompressedCoin::V1(CompressedCoinV1 {
            owner,
            amount,
            asset_id,
            tx_pointer: Default::default(), // purposely left blank
        })
    }
}
