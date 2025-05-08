use fuel_core_types::{
    entities::coins::coin::{
        CompressedCoin,
        CompressedCoinV1,
    },
    fuel_tx::{
        Address,
        AssetId,
        Input,
        UtxoId,
        Word,
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
    },
};

/// can either be a predicate coin or a signed coin
#[derive(Debug)]
enum Variant {
    Predicate,
    Signed,
}

#[derive(Debug)]
pub(crate) struct CoinInBatch {
    /// The utxo id
    utxo_id: UtxoId,
    /// The index of the transaction using this coin in the batch
    idx: usize,
    /// the owner of the coin
    owner: Address,
    /// the amount stored in the coin
    amount: Word,
    /// the asset the coin stores
    asset_id: AssetId,
    /// variant
    variant: Variant,
}

impl CoinInBatch {
    pub(crate) fn utxo(&self) -> &UtxoId {
        &self.utxo_id
    }

    pub(crate) fn idx(&self) -> usize {
        self.idx
    }

    pub(crate) fn from_signed_coin(signed_coin: &CoinSigned, idx: usize) -> Self {
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
            owner: *owner,
            amount: *amount,
            asset_id: *asset_id,
            variant: Variant::Signed,
        }
    }

    pub(crate) fn from_predicate_coin(
        predicate_coin: &CoinPredicate,
        idx: usize,
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
            owner: *owner,
            amount: *amount,
            asset_id: *asset_id,
            variant: Variant::Predicate,
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

impl From<&CoinInBatch> for Input {
    fn from(value: &CoinInBatch) -> Self {
        let CoinInBatch {
            utxo_id,
            owner,
            amount,
            asset_id,
            ..
        } = value;

        match value.variant {
            Variant::Signed => Input::CoinSigned(CoinSigned {
                utxo_id: *utxo_id,
                owner: *owner,
                amount: *amount,
                asset_id: *asset_id,
                ..Default::default()
            }),
            Variant::Predicate => Input::CoinPredicate(CoinPredicate {
                utxo_id: *utxo_id,
                owner: *owner,
                amount: *amount,
                asset_id: *asset_id,
                ..Default::default()
            }),
        }
    }
}
