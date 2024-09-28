mod collision;

use std::{
    collections::HashMap,
    time::Instant,
};

use fuel_core_types::{
    fuel_tx::{
        field::BlobId,
        Transaction,
        TxId,
    },
    fuel_vm::checked_transaction::Checked,
    services::txpool::PoolTransaction,
};
use num_rational::Ratio;
use tracing::instrument;

use crate::{
    collision_manager::{
        collision::SimpleCollision,
        Collision,
        CollisionManager,
    },
    config::Config,
    error::{
        CollisionReason,
        Error,
    },
    pool::collision::CollisionExt,
    ports::{
        AtomicView,
        TxPoolPersistentStorage,
    },
    selection_algorithms::{
        Constraints,
        SelectionAlgorithm,
    },
    storage::{
        RemovedTransactions,
        Storage,
    },
    verifications::FullyVerifiedTx,
};

/// The pool is the main component of the txpool service. It is responsible for storing transactions
/// and allowing the selection of transactions for inclusion in a block.
pub struct Pool<PSProvider, S: Storage, CM, SA> {
    /// Configuration of the pool.
    pub config: Config,
    /// The storage of the pool.
    storage: S,
    /// The collision manager of the pool.
    collision_manager: CM,
    /// The selection algorithm of the pool.
    selection_algorithm: SA,
    /// The persistent storage of the pool.
    persistent_storage_provider: PSProvider,
    /// Mapping from tx_id to storage_id.
    tx_id_to_storage_id: HashMap<TxId, S::StorageIndex>,
    /// Current pool gas stored.
    current_gas: u64,
    /// Current pool size in bytes.
    current_bytes_size: usize,
}

impl<PSProvider, S: Storage, CM, SA> Pool<PSProvider, S, CM, SA> {
    /// Create a new pool.
    pub fn new(
        persistent_storage_provider: PSProvider,
        storage: S,
        collision_manager: CM,
        selection_algorithm: SA,
        config: Config,
    ) -> Self {
        Pool {
            storage,
            collision_manager,
            selection_algorithm,
            persistent_storage_provider,
            config,
            tx_id_to_storage_id: HashMap::new(),
            current_gas: 0,
            current_bytes_size: 0,
        }
    }
}

impl<PS, View, S, CM, SA> Pool<PS, S, CM, SA>
where
    PS: AtomicView<LatestView = View>,
    View: TxPoolPersistentStorage,
    S: Storage,
    CM: CollisionManager<Storage = S, StorageIndex = S::StorageIndex>,
    SA: SelectionAlgorithm<Storage = S, StorageIndex = S::StorageIndex>,
{
    /// Insert transactions into the pool.
    /// Returns a list of results for each transaction.
    /// Each result is a list of transactions that were removed from the pool
    /// because of the insertion of the new transaction.
    #[instrument(skip(self))]
    pub fn insert(&mut self, tx: PoolTransaction) -> Result<Vec<PoolTransaction>, Error> {
        let latest_view = self
            .persistent_storage_provider
            .latest_view()
            .map_err(|e| Error::Database(format!("{:?}", e)))?;

        self.config.black_list.check_blacklisting(&tx)?;
        Self::check_blob_does_not_exist(&tx, &latest_view)?;
        self.storage
            .validate_inputs(&tx, &latest_view, self.config.utxo_validation)?;
        let collision = self.collision_manager.find_collision(&tx);
        let has_dependencies = self.storage.has_dependencies(&tx);

        if let Some(collision) = &collision {
            collision
                .check_collision_requirements(has_dependencies, &self.storage)
                .map_err(Error::Collided)?;
        }

        let mut removed_transactions = vec![];

        let can_fit_into_pool =
            self.can_fit_into_pool(&tx, &collision, has_dependencies)?;

        if let SpaceCheckResult::NotEnoughSpace(left) = can_fit_into_pool {
            let transactions_to_remove = self.find_free_space(left, &tx)?;

            for tx in transactions_to_remove {
                let removed = self.storage.remove_transaction_and_dependents_subtree(tx);
                self.update_components_and_caches_on_removal(removed.iter());
                removed_transactions.extend(removed);
            }
        }

        if let Some(collision) = &collision {
            for collided_tx in collision.colliding_transactions().keys() {
                let mut removed = self
                    .storage
                    .remove_transaction_and_dependents_subtree(*collided_tx);
                self.update_components_and_caches_on_removal(removed.iter());

                removed_transactions.extend(removed);
            }
        }
        drop(collision);

        let tx_id = tx.id();
        let gas = tx.max_gas();
        let creation_instant = Instant::now();
        let bytes_size = tx.metered_bytes_size();

        let storage_id = self.storage.store_transaction(tx, creation_instant)?;

        self.current_gas = self.current_gas.saturating_add(gas);
        self.current_bytes_size = self.current_bytes_size.saturating_add(bytes_size);
        self.tx_id_to_storage_id.insert(tx_id, storage_id);

        let tx =
            Storage::get(&self.storage, &storage_id).expect("Transaction is set above");
        self.collision_manager.on_stored_transaction(storage_id, tx);

        // No dependencies directly in the graph and the sorted transactions
        if !has_dependencies {
            self.selection_algorithm
                .new_executable_transaction(storage_id, tx);
        } else {
            self.selection_algorithm.on_stored_transaction(tx);
        }

        Ok(removed_transactions)
    }

    /// Check if a transaction can be inserted into the pool.
    pub fn can_insert_transaction(&self, tx: &PoolTransaction) -> Result<(), Error> {
        let persistent_storage = self
            .persistent_storage_provider
            .latest_view()
            .map_err(|e| Error::Database(format!("{:?}", e)))?;
        self.config.black_list.check_blacklisting(tx)?;
        Self::check_blob_does_not_exist(tx, &persistent_storage)?;
        self.storage.validate_inputs(
            tx,
            &persistent_storage,
            self.config.utxo_validation,
        )?;

        let has_dependencies = self.storage.has_dependencies(tx);

        let collision = self.collision_manager.find_collision(tx);
        if let Some(collision) = &collision {
            collision
                .check_collision_requirements(has_dependencies, &self.storage)
                .map_err(Error::Collided)?;
        }

        let can_fit_into_pool =
            self.can_fit_into_pool(tx, &collision, has_dependencies)?;

        if let SpaceCheckResult::NotEnoughSpace(left) = can_fit_into_pool {
            self.find_free_space(left, tx)?;
        }

        let collision_transaction =
            collision.map(|collision| collision.into_colliding_storage());

        self.storage
            .can_store_transaction(tx, collision_transaction)?;

        Ok(())
    }

    // TODO: Use block space also (https://github.com/FuelLabs/fuel-core/issues/2133)
    /// Extract transactions for a block.
    /// Returns a list of transactions that were selected for the block
    /// based on the constraints given in the configuration and the selection algorithm used.
    pub fn extract_transactions_for_block(
        &mut self,
    ) -> Result<Vec<PoolTransaction>, Error> {
        let extracted_transactions = self
            .selection_algorithm
            .gather_best_txs(
                Constraints {
                    max_gas: self.config.max_block_gas,
                },
                &self.storage,
            )?
            .into_iter()
            .map(|storage_id| {
                let storage_data = self
                    .storage
                    .remove_transaction_without_dependencies(storage_id)?;
                Ok(storage_data.transaction)
            })
            .collect::<Result<Vec<PoolTransaction>, Error>>()?;
        self.update_components_and_caches_on_removal(extracted_transactions.iter());
        Ok(extracted_transactions)
    }

    pub fn find_one(&self, tx_id: &TxId) -> Option<&PoolTransaction> {
        Storage::get(&self.storage, self.tx_id_to_storage_id.get(tx_id)?)
            .map(|data| &data.transaction)
    }

    /// Check if the pool has enough space to store a transaction.
    ///
    /// It returns `true` if:
    /// - Pool is not full
    /// - Removing colliding subtree is enough to make space
    ///
    /// It returns an error if the pool is full and transactions has dependencies.
    ///
    /// If none of the above is not true, return `false`.
    fn can_fit_into_pool(
        &self,
        tx: &PoolTransaction,
        collision: &Option<CM::Collision<'_>>,
        has_dependencies: bool,
    ) -> Result<SpaceCheckResult, Error> {
        let tx_gas = tx.max_gas();
        let bytes_size = tx.metered_bytes_size();
        let mut gas_left = self.current_gas.saturating_add(tx_gas);
        let mut bytes_left = self.current_bytes_size.saturating_add(bytes_size);
        let mut txs_left = self.storage.count().saturating_add(1);
        if gas_left <= self.config.pool_limits.max_gas
            && bytes_left <= self.config.pool_limits.max_bytes_size
            && txs_left <= self.config.pool_limits.max_txs
        {
            return Ok(SpaceCheckResult::EnoughSpace);
        }

        // If the transaction has a collision verify that by
        // removing the transaction we can free enough space
        // otherwise return an error
        if let Some(collision) = collision {
            for collision in collision.colliding_transactions().keys() {
                let Some(collision_data) = self.storage.get(collision) else {
                    debug_assert!(false, "Collision data not found");
                    tracing::warn!(
                        "Collision data not found in the storage during `can_fit_into_pool`."
                    );
                    continue
                };

                gas_left =
                    gas_left.saturating_sub(collision_data.dependents_cumulative_gas);
                bytes_left = bytes_left
                    .saturating_sub(collision_data.dependents_cumulative_bytes_size);
                txs_left =
                    txs_left.saturating_sub(collision_data.number_dependents_in_chain);
            }
        }

        if gas_left <= self.config.pool_limits.max_gas
            && bytes_left <= self.config.pool_limits.max_bytes_size
            && txs_left <= self.config.pool_limits.max_txs
        {
            return Ok(SpaceCheckResult::EnoughSpace);
        }

        // If the transaction has a dependency and the pool is full, we refuse it
        if has_dependencies {
            return Err(Error::NotInsertedLimitHit);
        }

        let left = NotEnoughSpace {
            gas_left,
            bytes_left,
            txs_left,
        };

        Ok(SpaceCheckResult::NotEnoughSpace(left))
    }

    /// Find free space in the pool by marking less profitable transactions for removal.
    ///
    /// Return the list of transactions that must be removed from the pool along all of
    /// their dependent subtree.
    ///
    /// Returns an error impossible to find enough space.
    fn find_free_space(
        &self,
        left: NotEnoughSpace,
        tx: &PoolTransaction,
    ) -> Result<Vec<S::StorageIndex>, Error> {
        let tx_gas = tx.max_gas();
        let mut gas_left = left.gas_left;
        let mut bytes_left = left.bytes_left;
        let mut txs_left = left.txs_left;

        let mut removed_transactions = vec![];

        // Here the transaction has no dependencies which means that it's an executable transaction
        // and we want to make space for it
        let new_tx_ratio = Ratio::new(tx.tip(), tx_gas);

        // We want to go over executable transactions and remove the less profitable ones.
        // It is imported to go over executable transactions, not dependent ones, because we
        // can include the same subtree several times in the calculation if we use dependent txs.
        let mut sorted_txs = self.selection_algorithm.get_less_worth_txs();

        while gas_left > self.config.pool_limits.max_gas
            || bytes_left > self.config.pool_limits.max_bytes_size
            || txs_left > self.config.pool_limits.max_txs
        {
            let storage_id = sorted_txs.next().ok_or(Error::NotInsertedLimitHit)?;
            let Some(storage_data) = self.storage.get(storage_id) else {
                debug_assert!(
                    false,
                    "Storage data not found for one of the less worth transactions"
                );
                tracing::warn!(
                    "Storage data not found for one of the less \
                    worth transactions during `find_free_space`."
                );
                continue
            };
            let ratio = Ratio::new(
                storage_data.dependents_cumulative_tip,
                storage_data.dependents_cumulative_gas,
            );

            if ratio > new_tx_ratio {
                return Err(Error::NotInsertedLimitHit);
            }

            gas_left = gas_left.saturating_sub(storage_data.dependents_cumulative_gas);
            bytes_left =
                bytes_left.saturating_sub(storage_data.dependents_cumulative_bytes_size);
            txs_left = txs_left.saturating_sub(storage_data.number_dependents_in_chain);

            removed_transactions.push(*storage_id);
        }

        Ok(removed_transactions)
    }

    fn check_blob_does_not_exist(
        tx: &PoolTransaction,
        persistent_storage: &impl TxPoolPersistentStorage,
    ) -> Result<(), Error> {
        if let PoolTransaction::Blob(checked_tx, _) = &tx {
            let blob_id = checked_tx.transaction().blob_id();
            if persistent_storage
                .blob_exist(blob_id)
                .map_err(|e| Error::Database(format!("{:?}", e)))?
            {
                return Err(Error::NotInsertedBlobIdAlreadyTaken(*blob_id))
            }
        }
        Ok(())
    }

    fn update_components_and_caches_on_removal<'a>(
        &mut self,
        mut removed_transactions: impl Iterator<Item = &'a PoolTransaction>,
    ) {
        for tx in removed_transactions {
            self.current_gas = self.current_gas.saturating_sub(tx.max_gas());
            self.current_bytes_size = self
                .current_bytes_size
                .saturating_sub(tx.metered_bytes_size());
            self.tx_id_to_storage_id.remove(&tx.id());
            self.collision_manager.on_removed_transaction(tx);
            self.selection_algorithm.on_removed_transaction(tx);
        }
    }
}

pub struct NotEnoughSpace {
    gas_left: u64,
    bytes_left: usize,
    txs_left: usize,
}

/// The result of the `can_fit_into_pool` check.
pub enum SpaceCheckResult {
    EnoughSpace,
    NotEnoughSpace(NotEnoughSpace),
}
