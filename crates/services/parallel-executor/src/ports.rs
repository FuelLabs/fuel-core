use std::collections::HashSet;

use fuel_core_types::fuel_tx::ContractId;
use fuel_core_upgradable_executor::native_executor::ports::MaybeCheckedTransaction;

pub trait TransactionsSource {
    /// Returns the a batch of transactions to satisfy the given parameters
    fn get_executable_transactions(
        &mut self,
        gas_limit: u64,
        tx_count_limit: u16,
        block_transaction_size_limit: u32,
        excluded_contract_ids: HashSet<ContractId>,
    ) -> Vec<MaybeCheckedTransaction>;
}
