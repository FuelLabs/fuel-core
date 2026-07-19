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

    /// Re-key this coin onto a different batch-local transaction index.
    ///
    /// Needed when a batch skips transactions: `coins_used` is built over the
    /// batch as DISPATCHED, while `coins_created` and the block indices the
    /// merge assigns are keyed to the transactions that actually COMMITTED.
    pub(crate) fn set_idx(&mut self, idx: usize) {
        self.idx = idx;
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
    /// Every coin created by an in-block transaction, keyed by `UtxoId` and
    /// carrying the BLOCK index of the transaction that created it.
    ///
    /// The block index — not the batch id — is what orders a creation against
    /// a use. A batch id only ever approximated it: it was a scheduling
    /// artifact that happened to be monotonic in block order because the
    /// scheduler was forced to dispatch a coin child after its parent's batch
    /// completed. Parallel VALIDATION drops that scheduling edge (a child's
    /// input carries the coin's own `owner`/`amount`/`asset_id`, so it never
    /// needed its parent to have executed), which lets a child land in an
    /// EARLIER batch than its parent. Block indices are known upfront from the
    /// block being validated, so they order creations against uses exactly and
    /// independently of how the work was scheduled.
    coins_registered: FxHashMap<UtxoId, (u32, CoinInBatch)>,
    /// The batch that CREATED each in-block coin. Only used to decide whether
    /// the merge needs a net-out (see [`Self::coins_needing_net_out`]); it has
    /// no bearing on validity, which is decided purely on block indices.
    creator_batch: FxHashMap<UtxoId, usize>,
    /// The batch that SPENT each coin, for the same purpose.
    spender_batch: FxHashMap<UtxoId, usize>,
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
            creator_batch: FxHashMap::default(),
            spender_batch: FxHashMap::default(),
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

    /// Register a batch's created coins against their BLOCK indices.
    ///
    /// `block_indices` maps this batch's transactions, by their batch-local
    /// index, onto their final index in the block. EVERY batch must be
    /// registered before ANY use is verified — a coin's creator may now be
    /// merged after its user (see [`Self::coins_registered`]).
    pub fn register_coins_created(
        &mut self,
        batch_id: usize,
        block_indices: &[u32],
        coins_created: Vec<CoinInBatch>,
    ) -> Result<(), SchedulerError> {
        for coin in coins_created {
            let block_index =
                block_indices.get(coin.idx()).copied().ok_or_else(|| {
                    SchedulerError::InternalError(format!(
                        "coin {} was created by batch-local tx {} but the batch \
                     carries only {} block indices",
                        coin.utxo(),
                        coin.idx(),
                        block_indices.len(),
                    ))
                })?;
            self.creator_batch.insert(*coin.utxo(), batch_id);
            self.coins_registered
                .insert(*coin.utxo(), (block_index, coin));
        }
        Ok(())
    }

    /// Coins whose creating batch merges AFTER their spending batch.
    ///
    /// The merge applies whole batches in batch-id order. A coin created and
    /// spent inside the block must end up ABSENT: sequentially the creating
    /// insert lands before the spending remove, so the remove wins. That still
    /// holds whenever the creator's batch comes first. But parallel VALIDATION
    /// no longer gates a coin child on its parent, so the SPENDER can merge
    /// first — and then the creator's insert lands last and the coin would
    /// survive into the final state and the state root.
    ///
    /// The merge cancels both writes of such a pair — see the net-out in
    /// `verify_coherency_and_merge_results`. Only INVERTED pairs are returned:
    /// an in-order pair's natural insert-then-remove is already correct, and
    /// touching it would corrupt the common case.
    pub fn coins_needing_net_out(&self) -> Vec<UtxoId> {
        let mut inverted: Vec<UtxoId> = self
            .spender_batch
            .iter()
            .filter(|(utxo, spender)| {
                self.creator_batch
                    .get(*utxo)
                    .is_some_and(|creator| creator > spender)
            })
            .map(|(utxo, _)| *utxo)
            .collect();
        // Deterministic order: the merge appends these as storage writes.
        inverted.sort_unstable();
        inverted
    }

    /// Verify a batch's spent coins. `block_indices` maps this batch's
    /// transactions, by batch-local index, onto their final block index.
    ///
    /// Call only after EVERY batch has been registered with
    /// [`Self::register_coins_created`].
    pub fn verify_coins_used<'a, S>(
        &mut self,
        batch_id: usize,
        block_indices: &[u32],
        coins_used: impl Iterator<Item = &'a CoinInBatch>,
        storage: &StorageTransaction<S>,
    ) -> Result<(), SchedulerError>
    where
        S: KeyValueInspect<Column = Column> + Send,
    {
        // Check if the coins used are not already used and if they are valid
        for coin in coins_used {
            let user_block_index =
                block_indices.get(coin.idx()).copied().ok_or_else(|| {
                    SchedulerError::InternalError(format!(
                        "coin {} is spent by batch-local tx {} but the batch \
                         carries only {} block indices",
                        coin.utxo(),
                        coin.idx(),
                        block_indices.len(),
                    ))
                })?;
            if self.coins_used.contains(coin.utxo()) {
                return Err(SchedulerError::InternalError(format!(
                    "Coin {} is already used in the batch",
                    coin.utxo(),
                )));
            }
            self.coins_used.insert(*coin.utxo());
            self.spender_batch.insert(*coin.utxo(), batch_id);
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
                        Some((creator_block_index, registered_coin)) => {
                            // Coin is created in the block. The creation must be
                            // ordered before this use IN THE BLOCK — a coin may
                            // only fund a transaction that comes after it.
                            //
                            // This compares BLOCK indices, which are fixed by the
                            // block itself, so the verdict does not depend on how
                            // the scheduler distributed the work: a child merged
                            // in an earlier batch than its parent is accepted iff
                            // it really does come later in the block, and a
                            // genuine "spends a coin created later" block is
                            // rejected however the batches happened to fall.
                            let created_before = *creator_block_index < user_block_index;
                            if !created_before {
                                return Err(SchedulerError::InternalError(format!(
                                    "Coin {} is spent by block transaction {} \
                                     but created by block transaction {} — a \
                                     coin cannot fund an earlier transaction",
                                    coin.utxo(),
                                    user_block_index,
                                    creator_block_index,
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

    // A coin may only fund a transaction that comes LATER IN THE BLOCK.
    #[test]
    fn creation_before_use_in_block_order_is_accepted() {
        let utxo = UtxoId::new([7u8; 32].into(), 0);
        let mut verifier = CoinDependencyChainVerifier::new(true);
        // created by block tx 5, spent by block tx 6
        verifier
            .register_coins_created(0, &[5], vec![coin_at(utxo, 0)])
            .unwrap();
        let storage = empty_storage();
        assert!(
            verifier
                .verify_coins_used(0, &[6], [coin_at(utxo, 0)].iter(), &storage)
                .is_ok(),
        );
    }

    // A coin created LATER in the block than the transaction spending it is a
    // genuine ordering violation and must be rejected.
    #[test]
    fn creation_after_use_in_block_order_is_rejected() {
        let utxo = UtxoId::new([10u8; 32].into(), 0);
        let mut verifier = CoinDependencyChainVerifier::new(true);
        // created by block tx 9, spent by block tx 4
        verifier
            .register_coins_created(0, &[9], vec![coin_at(utxo, 0)])
            .unwrap();
        let storage = empty_storage();
        assert!(
            verifier
                .verify_coins_used(0, &[4], [coin_at(utxo, 0)].iter(), &storage)
                .is_err(),
            "a coin cannot fund a transaction that precedes it in the block",
        );
    }

    // A coin spending itself — same block index for creator and user — is
    // rejected: the ordering is STRICT.
    #[test]
    fn same_block_index_is_rejected() {
        let utxo = UtxoId::new([12u8; 32].into(), 0);
        let mut verifier = CoinDependencyChainVerifier::new(true);
        verifier
            .register_coins_created(0, &[3], vec![coin_at(utxo, 0)])
            .unwrap();
        let storage = empty_storage();
        assert!(
            verifier
                .verify_coins_used(0, &[3], [coin_at(utxo, 0)].iter(), &storage)
                .is_err(),
        );
    }

    // THE CASE THIS CHANGE UNLOCKS: parallel validation no longer gates a coin
    // child on its parent, so the CHILD's batch may be verified before the
    // PARENT's batch is even registered — and both may merge in either order.
    // The verdict must follow BLOCK order, not batch order.
    #[test]
    fn child_in_an_earlier_batch_than_its_parent_is_accepted() {
        let utxo = UtxoId::new([13u8; 32].into(), 0);
        let storage = empty_storage();
        let mut verifier = CoinDependencyChainVerifier::new(true);

        // Parent is block tx 40 but landed in a LATER batch; child is block tx
        // 41 and landed in an EARLIER one. Registration of every batch happens
        // before any verification, which is what makes this decidable at all.
        verifier
            .register_coins_created(0, &[40], vec![coin_at(utxo, 0)])
            .unwrap();
        assert!(
            verifier
                .verify_coins_used(0, &[41], [coin_at(utxo, 0)].iter(), &storage)
                .is_ok(),
            "block order is satisfied, so the batch order must not matter",
        );
    }

    // The mirror image: a block that really does spend a coin before creating
    // it stays rejected even when the batches happen to fall in the order that
    // would have made the old batch-id check pass.
    #[test]
    fn batch_order_cannot_launder_a_block_order_violation() {
        let utxo = UtxoId::new([14u8; 32].into(), 0);
        let storage = empty_storage();
        let mut verifier = CoinDependencyChainVerifier::new(true);
        // Creator is block tx 41, user is block tx 40 — invalid block. The
        // creator was registered "first", which under the old batch-id rule
        // would have been accepted.
        verifier
            .register_coins_created(0, &[41], vec![coin_at(utxo, 0)])
            .unwrap();
        assert!(
            verifier
                .verify_coins_used(0, &[40], [coin_at(utxo, 0)].iter(), &storage)
                .is_err(),
        );
    }

    // ONLY an inverted pair — creator batch merging AFTER the spender batch —
    // needs the merge net-out. The in-order case already ends up absent because
    // the creating insert is applied before the spending remove; cancelling its
    // writes too would wrongly resurrect a coin the block really spent.
    #[test]
    fn only_inverted_create_spend_pairs_need_net_out() {
        let inverted = UtxoId::new([15u8; 32].into(), 0);
        let in_order = UtxoId::new([16u8; 32].into(), 0);
        let only_created = UtxoId::new([19u8; 32].into(), 0);
        let only_spent = UtxoId::new([17u8; 32].into(), 0);
        let storage = empty_storage();

        let mut verifier = CoinDependencyChainVerifier::new(false);
        // `in_order` + `only_created` are created by batch 0; `inverted` is
        // created by batch 7 — after the batch that spends it.
        verifier
            .register_coins_created(
                0,
                &[1],
                vec![coin_at(in_order, 0), coin_at(only_created, 0)],
            )
            .unwrap();
        verifier
            .register_coins_created(7, &[1], vec![coin_at(inverted, 0)])
            .unwrap();
        // Batch 3 spends all three spendable coins.
        verifier
            .verify_coins_used(
                3,
                &[2],
                [
                    coin_at(inverted, 0),
                    coin_at(in_order, 0),
                    coin_at(only_spent, 0),
                ]
                .iter(),
                &storage,
            )
            .unwrap();

        assert_eq!(
            verifier.coins_needing_net_out(),
            vec![inverted],
            "only the coin whose creator merges after its spender needs a \
             trailing remove",
        );
    }

    // A batch-local index with no matching block index is a bookkeeping bug,
    // not a valid block — surface it instead of silently mis-ordering.
    #[test]
    fn missing_block_index_is_an_error() {
        let utxo = UtxoId::new([18u8; 32].into(), 0);
        let storage = empty_storage();

        let mut verifier = CoinDependencyChainVerifier::new(true);
        assert!(
            verifier
                .register_coins_created(0, &[], vec![coin_at(utxo, 3)])
                .is_err(),
        );

        let mut verifier = CoinDependencyChainVerifier::new(true);
        assert!(
            verifier
                .verify_coins_used(0, &[0], [coin_at(utxo, 7)].iter(), &storage)
                .is_err(),
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
                .verify_coins_used(0, &[0], [coin_at(utxo, 0)].iter(), &storage)
                .is_err(),
            "strict mode must reject a coin that exists nowhere",
        );

        // Relaxed mode accepts...
        let mut relaxed = CoinDependencyChainVerifier::new(false);
        assert!(
            relaxed
                .verify_coins_used(0, &[0], [coin_at(utxo, 0)].iter(), &storage)
                .is_ok(),
            "relaxed mode must accept a coin that exists nowhere",
        );
        // ...but still tracks it for double-spend detection.
        assert!(
            relaxed
                .verify_coins_used(0, &[0, 1], [coin_at(utxo, 1)].iter(), &storage)
                .is_err(),
            "double-spend detection must stay active in relaxed mode",
        );
    }
}
