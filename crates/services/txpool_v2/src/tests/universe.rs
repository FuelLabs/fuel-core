use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
    fuel_tx::TransactionBuilder,
    fuel_vm::checked_transaction::builder::TransactionBuilderExt,
    services::txpool::PoolTransaction,
};

use crate::{
    config::Config, error::Error, pool::Pool
};

pub struct TxPoolUniverse {
    pool: Pool,
}

impl TxPoolUniverse {
    pub fn new() -> Self {
        TxPoolUniverse {
            pool: Pool::new(Config::default()),
        }
    }

    pub fn create_default_transaction(&self, tip: u64) -> PoolTransaction {
        PoolTransaction::Script(
            TransactionBuilder::script(vec![], vec![])
                .script_gas_limit(100000)
                .tip(tip)
                .finalize_checked_basic(Default::default()),
            ConsensusParametersVersion::MIN,
        )
    }

    pub fn insert_transactions_in_pool(
        &mut self,
        transactions: Vec<PoolTransaction>,
    ) -> Vec<Result<(), Error>> {
        self.pool.insert(transactions)
    }
}
