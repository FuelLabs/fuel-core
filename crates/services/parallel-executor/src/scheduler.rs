use std::{
    collections::HashSet,
    time::Duration,
};

use ::futures::{
    StreamExt,
    stream::FuturesUnordered,
};
use fuel_core_storage::transactional::{
    Changes,
    StorageChanges,
};
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_tx::{
        ContractId,
        UtxoId,
    },
};
use fuel_core_upgradable_executor::native_executor::ports::MaybeCheckedTransaction;
use tokio::runtime::Runtime;

use crate::ports::{
    Filter,
    TransactionFiltered,
    TransactionsSource,
};

/// The scheduler is responsible for managing the state of all the execution workers.
/// His goal is to gather transactions for the transaction source and organize their execution
/// through the different workers.
///
/// There is few rules that need to be followed in order to produce a valid execution result:
/// - The dependency chain of the input and output must be maintained across the block.
/// - The constraints of the block (maximum number of transactions, maximum size, maximum gas, etc.) must be respected.
///
/// Current design:
///
/// The scheduler creates multiple workers. For each of this workers, the scheduler will ask to the transaction source
/// to provide a batch of transactions that can be executed.
///
/// When a thread has finished his execution then it will notify the scheduler that will re-ask for a new batch to the transaction source.
/// This new batch mustn't contain any transaction that use a contract used in a batch of any other worker.
///
/// For transactions without contracts, they are treat them as independent transactions. A verification is done at the end for the coin dependency chain.
/// This can be done because we assume that the transaction pool is sending us transactions that are already correctly verified.
/// If we have a transaction that end up being skipped (only possible cause if consensus parameters changes) then we will have to
/// fallback a sequential execution of the transaction that used the skipped one as a dependency.

pub struct Config {
    block_gas_limit: u64,
    total_execution_time: Duration,
    block_transaction_size_limit: u32,
    block_transaction_count_limit: u16,
}

pub struct Scheduler<TxSource> {
    /// Config
    config: Config,
    /// The state of each worker
    workers_state: Vec<WorkerState>,
    /// Transaction source to ask for new transactions
    transaction_source: TxSource,
    /// Runtime to run the workers
    runtime: Option<Runtime>,
    /// Total maximum of transactions left
    tx_left: u16,
    /// Total maximum of byte size left
    tx_size_left: u32,
}

pub struct WorkerState {
    pub status: WorkerStatus,
    pub executing_contracts: Vec<ContractId>,
}

pub enum WorkerStatus {
    /// The worker is waiting for a new batch of transactions
    Idle,
    /// The worker is currently executing a batch of transactions
    Executing,
    /// The worker is merging his state with another
    Merging,
}

// Shutdown the tokio runtime to avoid panic if executor is already
//   used from another tokio runtime
impl<TxSource> Drop for Scheduler<TxSource> {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl<TxSource> Scheduler<TxSource>
where
    TxSource: TransactionsSource,
{
    pub fn new(
        config: Config,
        number_of_worker: usize,
        transaction_source: TxSource,
    ) -> Self {
        let mut workers_state = Vec::with_capacity(number_of_worker);
        for _ in 0..number_of_worker {
            workers_state.push(WorkerState {
                status: WorkerStatus::Idle,
                executing_contracts: vec![],
            });
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(number_of_worker)
            .enable_all()
            .build()
            .unwrap();

        Self {
            workers_state,
            transaction_source,
            runtime: Some(runtime),
            tx_left: config.block_transaction_count_limit,
            tx_size_left: config.block_transaction_size_limit,
            config,
        }
    }

    // TODO: Error type
    pub async fn run(&mut self) -> Result<StorageChanges, String> {
        let runtime = self.runtime.as_ref().unwrap();
        let now = tokio::time::Instant::now();
        let mut transactions_left_to_fetch = true;
        let new_tx_notifier = self.transaction_source.get_new_transactions_notifier();
        let deadline = now + self.config.total_execution_time;
        let mut storage_changes = vec![];
        // Store the futures of all the workers to be triggered when one of them finish
        let mut futures = FuturesUnordered::new();

        // All workers starts empty, so we fetch the first batch
        let number_of_workers = self.workers_state.len();
        let initial_gas = self
            .config
            .block_gas_limit
            .checked_div(number_of_workers as u64)
            .ok_or("Invalid block gas limit")?;
        for (i, state) in self.workers_state.iter_mut().enumerate() {
            let (batch, _) = self.transaction_source.get_executable_transactions(
                initial_gas,
                self.tx_left,
                self.tx_size_left,
                Filter {
                    excluded_contract_ids: HashSet::new(),
                },
            );

            // TODO: Maybe it's the transaction source that should gather these infos and the full size
            let (contracts_used, coins_used) = get_contracts_and_coins_used(&batch);
            self.tx_left -= batch.len() as u16;
            self.tx_size_left -= batch.iter().map(|tx| tx.size()).sum::<usize>() as u32;

            if batch.is_empty() {
                // No more transactions to execute
                // (none can be filtered out because we are the first batch)
                break;
            }

            futures.push(runtime.spawn({
                let worker_id = i;
                async move {
                    // TODO: Execute the batch of transactions
                    (worker_id, Changes::default(), coins_used)
                }
            }));
            state.executing_contracts.extend(contracts_used);

            state.status = WorkerStatus::Executing;
        }
        // Waiting for the workers to notify us
        'outer: loop {
            tokio::select! {
                // We have new transactions to execute
                _ = new_tx_notifier.notified() => {
                    transactions_left_to_fetch = true;
                }
                result = futures.next() => {
                    match result {
                        Some(Ok((worker_id, changes, _))) => {
                            // Update the state of the worker
                            self.workers_state[worker_id].status = WorkerStatus::Idle;
                            self.workers_state[worker_id].executing_contracts.clear();
                            storage_changes.push(changes);
                            if !transactions_left_to_fetch {
                                // We have no more transactions to fetch
                                continue 'outer;
                            }
                            // TODO: Avoid code duplication
                            let contracts_currently_used = self
                                .workers_state
                                .iter()
                                .flat_map(|state| state.executing_contracts.clone())
                                .collect::<HashSet<_>>();
                            let spent_time = now.elapsed();
                            // Time left in percentage to have the gas percentage left
                            // TODO: maybe try to remove "as"
                            let current_gas = initial_gas * ((1u128 - spent_time.as_millis() / self.config.total_execution_time.as_millis()) as u64);
                            let (batch, filtered) = self.transaction_source.get_executable_transactions(
                                current_gas,
                                self.tx_left,
                                self.tx_size_left,
                                Filter {
                                    excluded_contract_ids: contracts_currently_used,
                                },
                            );

                            let (contracts_used, coins_used) = get_contracts_and_coins_used(&batch);

                            if batch.is_empty() {
                                if filtered == TransactionFiltered::Filtered {
                                    // We have filtered out some transactions, they will need to be fetched
                                    // by another worker
                                    continue 'outer;
                                } else {
                                    // No more transactions in tx pool don't ask until the notifier tells us
                                    transactions_left_to_fetch = false;
                                    continue 'outer;
                                }
                            }

                            futures.push(runtime.spawn({
                                let worker_id = worker_id;
                                async move {
                                    // TODO: Execute the batch of transactions
                                    (worker_id, Changes::default(), coins_used)
                                }
                            }));
                            self.workers_state[worker_id].executing_contracts = contracts_used;
                            self.workers_state[worker_id].status = WorkerStatus::Executing;
                        }
                        _ => {
                            return Err("Worker failed".to_string());
                        }
                    }
                }
                // We have reached the deadline
                _ = tokio::time::sleep_until(deadline) => {
                    // We have reached the deadline
                    break 'outer;
                }
            }
        }
        let tolerance_execution_time_overflow = self.config.total_execution_time / 10;
        let now = tokio::time::Instant::now();

        // We have reached the deadline
        // We need to merge the states of all the workers
        for future in futures {
            match future.await {
                Ok((_, changes, _)) => {
                    // TODO: Be careful ordering
                    storage_changes.push(changes);
                }
                Err(_) => {
                    return Err("Worker failed".to_string());
                }
            }
        }

        if now.elapsed() > tolerance_execution_time_overflow {
            tracing::warn!(
                "Execution time exceeded the limit by: {}ms",
                now.elapsed().as_millis()
            );
        }

        // TODO: Verify the coin dependency chain

        Ok(StorageChanges::ChangesList(storage_changes))
    }
}

fn get_contracts_and_coins_used(
    batch: &[MaybeCheckedTransaction],
) -> (Vec<ContractId>, Vec<UtxoId>) {
    let mut contracts_used = vec![];
    let mut coins_used = vec![];

    for tx in batch {
        let inputs = tx.inputs();
        for input in inputs.iter() {
            match input {
                fuel_core_types::fuel_tx::Input::Contract(contract) => {
                    contracts_used.push(contract.contract_id);
                }
                fuel_core_types::fuel_tx::Input::CoinSigned(coin) => {
                    coins_used.push(coin.utxo_id);
                }
                fuel_core_types::fuel_tx::Input::CoinPredicate(coin) => {
                    coins_used.push(coin.utxo_id);
                }
                _ => {}
            }
        }
    }

    (contracts_used, coins_used)
}
