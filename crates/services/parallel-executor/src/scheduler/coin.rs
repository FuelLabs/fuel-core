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
        TxPointer,
        UtxoId,
        Word,
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
    },
};
use fxhash::{
    FxHashMap,
    FxHashSet,
};

use super::SchedulerError;

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
    coins_used: FxHashSet<UtxoId>,
    /// The stored `tx_pointer` of every spent coin found in the DATABASE while
    /// verifying `coins_used`. Captured here so the post-merge coin-input
    /// `TxPointer` normalization can reuse the coins this verifier already
    /// loaded instead of re-reading them from storage.
    db_coin_pointers: FxHashMap<UtxoId, TxPointer>,
    /// The node's `utxo_validation` flag. When `false` (relaxed/debugging
    /// mode), the sequential executor accepts coin inputs that exist neither
    /// in the database nor in the block (`get_coin_or_default` fabricates
    /// them), so this verifier must not reject them either. ONLY the
    /// "must exist in db or block" rejection is relaxed: double-spend
    /// detection and the cross-batch ordering/equality checks on coins that
    /// ARE tracked stay active in both modes, because the state merge relies
    /// on them.
    utxo_validation: bool,
}

impl CoinDependencyChainVerifier {
    pub fn new(utxo_validation: bool) -> Self {
        Self {
            coins_registered: FxHashMap::default(),
            coins_used: FxHashSet::default(),
            db_coin_pointers: FxHashMap::default(),
            utxo_validation,
        }
    }

    /// The stored `tx_pointer` of every DATABASE coin loaded during
    /// [`Self::verify_coins_used`], keyed by `UtxoId`. Consumes the verifier
    /// (it is only called after the last batch is verified).
    pub fn into_db_coin_pointers(self) -> FxHashMap<UtxoId, TxPointer> {
        self.db_coin_pointers
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
        &mut self,
        batch_id: usize,
        coins_used: impl Iterator<Item = &'a CoinInBatch>,
        storage: &StorageTransaction<S>,
    ) -> Result<(), SchedulerError>
    where
        S: KeyValueInspect<Column = Column> + Send,
    {
        // Check if the coins used are not already used and if they are valid
        for coin in coins_used {
            if self.coins_used.contains(coin.utxo()) {
                return Err(SchedulerError::InternalError(format!(
                    "Coin {} is already used in the batch",
                    coin.utxo(),
                )));
            }
            self.coins_used.insert(*coin.utxo());
            match storage.storage::<Coins>().get(coin.utxo()) {
                Ok(Some(db_coin)) => {
                    // Coin is in the database
                    match coin.equal_compressed_coin(&db_coin) {
                        true => {
                            self.db_coin_pointers
                                .insert(*coin.utxo(), *db_coin.tx_pointer());
                            continue
                        }
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
                            // Coin is created in the block. The creation must be
                            // ordered before this use:
                            // * a STRICTLY EARLIER batch is always ordered first
                            //   — the per-tx `idx` is batch-local, so comparing a
                            //   creator's idx in batch `i` against a user's idx in
                            //   batch `j > i` is meaningless (a coin created at
                            //   idx 5 of batch 0 legitimately funds a tx at idx 0
                            //   of batch 1). The old code ANDed the idx check in
                            //   unconditionally and wrongly rejected such
                            //   cross-batch coins as "created in a later batch".
                            // * within the SAME batch the creator tx must come at
                            //   an earlier-or-equal index than the user tx.
                            let created_before = *coin_creation_batch_id < batch_id
                                || (*coin_creation_batch_id == batch_id
                                    && registered_coin.idx() <= coin.idx());
                            if !created_before {
                                // Coin is created in a batch that is after the current one
                                return Err(SchedulerError::InternalError(format!(
                                    "Coin {} is created in a batch that is after the current one",
                                    coin.utxo()
                                )));
                            }
                            if registered_coin != coin {
                                return Err(SchedulerError::InternalError(format!(
                                    "coin is invalid: {}",
                                    coin.utxo(),
                                )));
                            }
                            // Coin is created earlier in the block and matches.
                            continue;
                        }
                        None => {
                            if self.utxo_validation {
                                return Err(SchedulerError::InternalError(format!(
                                    "Coin {} is not in the database and not created in the block",
                                    coin.utxo(),
                                )));
                            }
                            // `utxo_validation` is off: match the sequential
                            // executor, which fabricates the missing coin from
                            // the input's own fields (`get_coin_or_default`)
                            // instead of rejecting it. The coin was already
                            // recorded in `coins_used` above, so the
                            // double-spend bookkeeping is unaffected.
                            continue;
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

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::{
        structured_storage::test::InMemoryStorage,
        transactional::{
            IntoTransaction,
            StorageTransaction,
        },
    };

    fn coin_at(utxo: UtxoId, idx: usize) -> CoinInBatch {
        CoinInBatch::from_output(
            utxo,
            idx,
            *utxo.tx_id(),
            Address::default(),
            100,
            AssetId::default(),
        )
    }

    fn empty_storage() -> StorageTransaction<InMemoryStorage<Column>> {
        InMemoryStorage::<Column>::default().into_transaction()
    }

    // Regression (found by the sequential-replay oracle): a coin created in a
    // STRICTLY EARLIER batch must be accepted regardless of the batch-local
    // indices. Creator at idx 5 of batch 0 funding a user at idx 0 of batch 1 is
    // valid; the old `registered.idx() <= used.idx()` conjunction wrongly
    // rejected it as "created in a later batch".
    #[test]
    fn earlier_batch_coin_is_accepted_even_when_creator_idx_is_higher() {
        let utxo = UtxoId::new([7u8; 32].into(), 0);
        let mut verifier = CoinDependencyChainVerifier::new(true);
        verifier.register_coins_created(0, vec![coin_at(utxo, 5)]);
        let storage = empty_storage();
        let used = coin_at(utxo, 0);
        assert!(
            verifier
                .verify_coins_used(1, [used].iter(), &storage)
                .is_ok(),
            "a coin created in an earlier batch must be accepted regardless of idx",
        );
    }

    // Within the SAME batch, the creator must come at an earlier-or-equal index.
    #[test]
    fn same_batch_requires_creator_before_user() {
        let utxo = UtxoId::new([8u8; 32].into(), 0);
        let mut verifier = CoinDependencyChainVerifier::new(true);
        verifier.register_coins_created(0, vec![coin_at(utxo, 2)]);
        let storage = empty_storage();
        // creator idx 2 <= user idx 5 → ok
        assert!(
            verifier
                .verify_coins_used(0, [coin_at(utxo, 5)].iter(), &storage)
                .is_ok(),
        );

        let utxo2 = UtxoId::new([9u8; 32].into(), 0);
        let mut verifier = CoinDependencyChainVerifier::new(true);
        verifier.register_coins_created(0, vec![coin_at(utxo2, 5)]);
        let storage = empty_storage();
        // creator idx 5 > user idx 1 in the same batch → reject
        assert!(
            verifier
                .verify_coins_used(0, [coin_at(utxo2, 1)].iter(), &storage)
                .is_err(),
        );
    }

    // A coin created in a LATER batch than its use is a genuine ordering
    // violation and must still be rejected.
    #[test]
    fn later_batch_coin_is_rejected() {
        let utxo = UtxoId::new([10u8; 32].into(), 0);
        let mut verifier = CoinDependencyChainVerifier::new(true);
        verifier.register_coins_created(2, vec![coin_at(utxo, 0)]);
        let storage = empty_storage();
        assert!(
            verifier
                .verify_coins_used(1, [coin_at(utxo, 0)].iter(), &storage)
                .is_err(),
            "a coin created in a later batch must be rejected",
        );
    }

    // `utxo_validation = false` (relaxed/debugging mode): a coin that exists
    // neither in the database nor in the block must be ACCEPTED, matching the
    // sequential executor's `get_coin_or_default` fabrication. Strict mode
    // keeps rejecting it (see `execute__utxo_validation_on_still_rejects_...`
    // for the end-to-end pin), and double-spend detection stays active even in
    // relaxed mode.
    #[test]
    fn unknown_coin_accepted_only_when_utxo_validation_off() {
        let utxo = UtxoId::new([11u8; 32].into(), 0);
        let storage = empty_storage();

        // Strict mode rejects.
        let mut strict = CoinDependencyChainVerifier::new(true);
        assert!(
            strict
                .verify_coins_used(0, [coin_at(utxo, 0)].iter(), &storage)
                .is_err(),
            "strict mode must reject a coin that exists nowhere",
        );

        // Relaxed mode accepts...
        let mut relaxed = CoinDependencyChainVerifier::new(false);
        assert!(
            relaxed
                .verify_coins_used(0, [coin_at(utxo, 0)].iter(), &storage)
                .is_ok(),
            "relaxed mode must accept a coin that exists nowhere",
        );
        // ...but still tracks it for double-spend detection.
        assert!(
            relaxed
                .verify_coins_used(1, [coin_at(utxo, 1)].iter(), &storage)
                .is_err(),
            "double-spend detection must stay active in relaxed mode",
        );
    }
}
