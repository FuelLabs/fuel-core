use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::services::txpool::PoolTransaction;
use tracing::instrument;

use crate::{
    collision_manager::CollisionManager,
    config::Config,
    error::Error,
    ports::TxPoolDb,
    selection_algorithms::{
        Constraints,
        SelectionAlgorithm,
    },
    storage::Storage,
};

pub struct Pool<DB, S, CM, SA> {
    storage: S,
    collision_manager: CM,
    selection_algorithm: SA,
    db: DB,
    pub config: Config,
}

impl<DB, S, CM, SA> Pool<DB, S, CM, SA> {
    pub fn new(
        database: DB,
        storage: S,
        collision_manager: CM,
        selection_algorithm: SA,
        config: Config,
    ) -> Self {
        Pool {
            storage,
            collision_manager,
            selection_algorithm,
            db: database,
            config,
        }
    }
}

impl<DB, View, S, CM, SA> Pool<DB, S, CM, SA>
where
    DB: AtomicView<LatestView = View>,
    View: TxPoolDb,
    S: Storage,
    CM: CollisionManager<S>,
    SA: SelectionAlgorithm<S>,
{
    #[instrument(skip(self))]
    pub fn insert(
        &mut self,
        transactions: Vec<PoolTransaction>,
    ) -> Result<Vec<Result<Vec<PoolTransaction>, Error>>, Error> {
        let db_view = self
            .db
            .latest_view()
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(transactions
            .into_iter()
            .map(|tx| {
                if self.storage.count() >= self.config.max_txs {
                    return Err(Error::NotInsertedLimitHit);
                }
                self.config.black_list.check_blacklisting(&tx)?;
                let collisions = self
                    .collision_manager
                    .collect_tx_collisions(&tx, &self.storage)?;
                let dependencies = self.storage.collect_dependencies_transactions(
                    &tx,
                    collisions.reasons,
                    &db_view,
                    self.config.utxo_validation,
                )?;
                let has_dependencies = !dependencies.is_empty();
                let (storage_id, removed_transactions) = self.storage.store_transaction(
                    tx,
                    dependencies,
                    collisions.colliding_txs,
                )?;
                // No dependencies directly in the graph and the sorted transactions
                if !has_dependencies {
                    self.selection_algorithm
                        .new_executable_transactions(vec![storage_id], &self.storage)?;
                }
                let tx = self.storage.get(&storage_id)?;
                let result = removed_transactions
                    .into_iter()
                    .map(|tx| {
                        self.collision_manager.on_removed_transaction(&tx)?;
                        self.selection_algorithm.on_removed_transaction(&tx)?;
                        Ok(tx)
                    })
                    .collect();
                self.collision_manager
                    .on_stored_transaction(&tx.transaction, storage_id)?;
                result
            })
            .collect())
    }

    // TODO: Use block space also
    pub fn extract_transactions_for_block(
        &mut self,
    ) -> Result<Vec<PoolTransaction>, Error> {
        self.selection_algorithm.gather_best_txs(
            Constraints {
                max_gas: self.config.max_block_gas,
            },
            &mut self.storage,
        )
    }

    pub fn prune(&mut self) -> Result<Vec<PoolTransaction>, Error> {
        Ok(vec![])
    }
}
