use crate::{
    txpool_v2::{
        collision_detector::{
            collision_reasons,
            CollisionDetector,
            CollisionReason,
        },
        executable_transactions::RemovedTransactions,
    },
    types::TxId,
    TxInfo,
};
use fuel_core_types::{
    fuel_tx::UtxoId,
    services::txpool::{
        ArcPoolTx,
        Error,
    },
};
use num_rational::Ratio;
use std::collections::{
    HashMap,
    HashSet,
};

/// Transactions create a dependency graph where each input is an edge.
///
/// In the example below each `.` is an output of a transaction.
///
///      Tx1[.     .      .]    Tx6[.]  Tx9[..]
///         /      |       \        |  /     |
/// Tx2[. .]   Tx3[.]  Tx4[..]  Tx7[.]  Tx10[.]
///        \             /   \     /         |
///         Tx5[. . . .]     tx8[.]     Tx11[.]
struct DependentTransaction {
    info: TxInfo,
    unresolved_coins: HashSet<UtxoId>,
    parents: HashSet<TxId>,
    children: HashSet<TxId>,
    /// The number of edges to all parents of a transaction in the graph.
    /// In the example above:
    /// - Tx1, Tx6, Tx9 have 0 edges to parents.
    /// - Tx2, Tx3, Tx4, Tx10 have 1 edge to parents.
    /// - Tx7, Tx11 have 2 edges to parents.
    /// - Tx5 has 4 edges to parents.
    /// - Tx8 has 5 edges to parents.
    edges_to_parents: usize,
    /// The number of edges to all children of a transaction in the graph.
    /// In the example above:
    /// - Tx3, Tx5, Tx8, Tx11 have 0 edges to children.
    /// - Tx2, Tx4, Tx7, Tx10 have 1 edge to children.
    /// - Tx6 has 2 edges to children.
    /// - Tx9 has 4 edges to children.
    /// - Tx1 has 6 edges to children.
    edges_of_children: usize,
    /// The cumulative tip of a transaction and all of its children.
    cumulative_tip: u64,
    /// The cumulative gas of a transaction and all of its children.
    cumulative_gas: u64,
}

pub struct DependentTransactions {
    /// The maximum number of edges to children and parents for the dependent transaction.
    max_edge_number: usize,
    // TODO: Consider using an array with many transaction slots
    //  and use the index to iterate over transactions.
    tx_info: HashMap<TxId, DependentTransaction>,
    /// Dependency graph of transactions where the key is an output(coin)
    /// and value is a dependent transaction.
    ///
    /// The graph doesn't allow two transactions to spend the same coin;
    /// conflicts are resolved during insertion in favor of a more profitable transaction.
    dependencies_graph: HashMap<CollisionReason, TxId>,
    /// Transactions stay in `DependentTransactions` until all of
    /// their coin inputs are resolved(confirmed by the executor).
    ///
    /// The key is a coin that is not resolved yet, and the value is a
    /// transaction that depends on it.
    unresolved_coins: HashMap<UtxoId, TxId>,
}

impl DependentTransactions {
    pub fn insert(
        &mut self,
        info: TxInfo,
        unresolved_coins: Vec<UtxoId>,
    ) -> Result<RemovedTransactions, Error> {
        let collided_transactions = self.check_collisions(&info.tx)?;
    }

    fn check_collisions(&mut self, tx: &ArcPoolTx) -> Result<Vec<TxId>, Error> {
        let new_tx_ratio = Ratio::new(tx.tip(), tx.max_gas());
        let collided_transactions: HashSet<TxId> = collision_reasons(tx)
            .filter_map(|collision_reason| {
                self.dependencies_graph.get(&collision_reason).cloned()
            })
            .collect();

        let (total_tip, total_gas) = collided_transactions.iter().fold(
            (0u64, 0u64),
            |(total_tip, total_gas), collision_reason| {
                let dependent_tx = self
                    .tx_info
                    .get(collision_reason)
                    .expect("Transaction always should exist in `tx_info`");
                let total_tip = total_tip.saturating_add(dependent_tx.cumulative_tip);
                let total_gas = total_gas.saturating_add(dependent_tx.cumulative_gas);
                (total_tip, total_gas)
            },
        );

        let collision_tx_ratio = Ratio::new(total_tip, total_gas);

        if new_tx_ratio <= collision_tx_ratio {
            return Err(Error::NotInsertedBecauseCollidedTransactionsMoreProfitable);
        }

        Ok(collided_transactions)
    }
}
