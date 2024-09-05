use std::{
    collections::HashMap,
    time::Duration,
};

use fuel_core_types::{
    fuel_tx::{
        Output,
        TxId,
    },
    services::txpool::PoolTransaction,
};
use indextree::{
    Arena, Node, NodeId
};
use tracing::instrument;

use crate::{
    config::Config, error::Error, registries::Registries
};

struct TreeNode {
    transaction: PoolTransaction,
    depth: usize,
    // Sum of the tip of this transaction and all the children included in a single block
    profitability_subtree: u64,
}

type Parents = Vec<TxId>;

pub struct Pool {
    // TODO: Change to use a graph
    tree: Arena<TreeNode>,
    most_profitable_subtree: Option<NodeId>,
    node_tx_index: HashMap<TxId, NodeId>,
    registries: Registries,
    config: Config
}

impl Pool {
    pub fn new(config: Config) -> Self {
        Pool {
            tree: Arena::new(),
            most_profitable_subtree: None,
            node_tx_index: HashMap::default(),
            registries: Default::default(),
            config
        }
    }

    fn get_node_by_tx_id(&self, tx_id: TxId) -> Result<&Node<TreeNode>, Error> {
        let node_id = self.node_tx_index.get(&tx_id).ok_or(Error::TransactionNotFound(format!(
            "Transaction {} doesn't exist in the TxPool", tx_id
        )))?;
        self.tree.get(*node_id).ok_or(Error::TransactionNotFound(format!(
            "Transaction {} doesn't exist in the TxPool", tx_id
        )))
    }

    #[instrument(skip(self))]
    pub fn insert(
        &mut self,
        transactions: Vec<PoolTransaction>,
    ) -> Vec<Result<(), Error>> {
        transactions
            .into_iter()
            .map(|tx| {
                let (parents, registries) = self.prepare_inclusion(&tx)?;
                for p_id in parents {
                    let node = self.get_node_by_tx_id(p_id)?;
                    if node.get().depth == self.config.max_depth {
                        return Err(Error::NotInsertedMaxDepth)
                    }
                }
                self.registries.extend(registries);
                // TODO: Add node to the tree
                Ok(())
            })
            .collect()
    }

    // 
    fn prepare_inclusion(
        &self,
        tx: &PoolTransaction,
    ) -> Result<(Parents, Registries), Error> {
        todo!()
    }


    pub fn extract_transactions_for_block(
        &mut self,
    ) -> Result<Vec<PoolTransaction>, Error> {
        Ok(vec![])
    }

    pub fn prune(
        &mut self,
        tx_ttl: Duration,
    ) -> Result<Vec<PoolTransaction>, Error> {
        Ok(vec![])
    }
}
