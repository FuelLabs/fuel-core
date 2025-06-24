use fuel_core_storage::{
    StorageAsRef,
    column::Column,
    kv_store::KeyValueInspect,
    tables::Coins,
    transactional::StorageTransaction,
};
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
use fxhash::FxHashMap;

use crate::scheduler::SchedulerError;
#[derive(Debug, Eq)]
pub(crate) struct CoinInBatch {
    /// The utxo id
    utxo_id: UtxoId,
    /// The index of the transaction using this coin in the batch
    idx: usize,
    /// The TxId that use this coin (useful to remove them from the batch in case of skipped tx)
    tx_id: TxId,
    /// the owner of the coin
    owner: Address,
    /// the amount stored in the coin
    amount: Word,
    /// the asset the coin stores
    asset_id: AssetId,
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

    pub(crate) fn tx_id(&self) -> &TxId {
        &self.tx_id
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

    pub(crate) fn from_output(
        utxo_id: UtxoId,
        idx: usize,
        tx_id: TxId,
        owner: Address,
        amount: Word,
        asset_id: AssetId,
    ) -> Self {
        CoinInBatch {
            utxo_id,
            idx,
            tx_id,
            owner,
            amount,
            asset_id,
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

pub struct CoinDependencyChainVerifier {
    coins_registered: FxHashMap<UtxoId, (usize, CoinInBatch)>,
}

impl CoinDependencyChainVerifier {
    pub fn new() -> Self {
        Self {
            coins_registered: FxHashMap::default(),
        }
    }

    pub fn register_coins_created(
        &mut self,
        batch_id: usize,
        coins_created: Vec<CoinInBatch>,
    ) {
        for coin in coins_created {
            self.coins_registered.insert(*coin.utxo(), (batch_id, coin));
        }
    }

    pub fn verify_coins_used<'a, S>(
        &self,
        batch_id: usize,
        coins_used: impl Iterator<Item = &'a CoinInBatch>,
        storage: &StorageTransaction<S>,
    ) -> Result<(), SchedulerError>
    where
        S: KeyValueInspect<Column = Column> + Send,
    {
        for coin in coins_used {
            match storage.storage::<Coins>().get(coin.utxo()) {
                Ok(Some(db_coin)) => {
                    // Coin is in the database
                    match coin.equal_compressed_coin(&db_coin) {
                        true => continue,
                        false => {
                            return Err(SchedulerError::InternalError(format!(
                                "coin is invalid: {}",
                                coin.utxo(),
                            )));
                        }
                    }
                }
                Ok(None) => {
                    // Coin is not in the database
                    match self.coins_registered.get(coin.utxo()) {
                        Some((coin_creation_batch_id, registered_coin)) => {
                            // Coin is in the block
                            if coin_creation_batch_id <= &batch_id
                                && registered_coin.idx() <= coin.idx()
                                && registered_coin == coin
                            {
                                // Coin is created in a batch that is before the current one
                                continue;
                            } else {
                                // Coin is created in a batch that is after the current one
                                return Err(SchedulerError::InternalError(format!(
                                    "Coin {} is created in a batch that is after the current one",
                                    coin.utxo()
                                )));
                            }
                        }
                        None => {
                            return Err(SchedulerError::InternalError(format!(
                                "Coin {} is not in the database and not created in the block",
                                coin.utxo(),
                            )));
                        }
                    }
                }
                Err(e) => {
                    return Err(SchedulerError::InternalError(format!(
                        "Error while getting coin {}: {e}",
                        coin.utxo(),
                    )));
                }
            }
        }
        Ok(())
    }
}
