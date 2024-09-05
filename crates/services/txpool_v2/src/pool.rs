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
use petgraph::Graph;
use tracing::instrument;

use crate::{
    config::Config,
    error::Error,
    registries::Registries,
};

struct GraphNode {
    transaction: PoolTransaction
}

type Parents = Vec<TxId>;

pub struct Pool {
    // TODO: Change to use a graph
    graph: Graph<GraphNode, ()>,
    registries: Registries,
    config: Config,
}

impl Pool {
    pub fn new(config: Config) -> Self {
        Pool {
            graph: Graph::new(),
            registries: Default::default(),
            config,
        }
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
                let node_idx = self.graph.add_node(GraphNode {
                    transaction: tx,
                });
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

    pub fn prune(&mut self, tx_ttl: Duration) -> Result<Vec<PoolTransaction>, Error> {
        Ok(vec![])
    }
}
