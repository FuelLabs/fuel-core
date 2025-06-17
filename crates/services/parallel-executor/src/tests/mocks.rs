use std::{
    collections::VecDeque,
    sync::{
        Arc,
        Mutex,
    },
};

use fuel_core_executor::ports::{
    PreconfirmationSenderPort,
    RelayerPort,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::{
        ConsensusParameters,
        Transaction,
    },
    fuel_vm::checked_transaction::{
        CheckedTransaction,
        IntoChecked,
    },
    services::preconfirmation::Preconfirmation,
};

use crate::ports::{
    Filter,
    TransactionFiltered,
    TransactionSourceExecutableTransactions,
    TransactionsSource,
};

#[derive(Debug, Clone)]
pub struct MockRelayer;

impl RelayerPort for MockRelayer {
    fn enabled(&self) -> bool {
        true
    }

    fn get_events(
        &self,
        _da_height: &DaBlockHeight,
    ) -> anyhow::Result<Vec<fuel_core_types::services::relayer::Event>> {
        Ok(vec![])
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PoolRequestParams {
    pub gas_limit: u64,
    pub tx_count_limit: u16,
    pub block_transaction_size_limit: u64,
    pub filter: Filter,
}

pub struct MockTransactionsSource {
    response_queue: Arc<Mutex<VecDeque<MockTxPoolResponse>>>,
}

#[derive(Debug, Clone)]
pub struct MockTxPoolResponse {
    pub transactions: Vec<CheckedTransaction>,
    pub filtered: TransactionFiltered,
    pub filter: Option<Filter>,
    pub gas_limit_lt: Option<u64>,
}

impl MockTxPoolResponse {
    pub fn new(transactions: &[&Transaction], filtered: TransactionFiltered) -> Self {
        Self {
            transactions: into_checked_txs(transactions),
            filtered,
            filter: None,
            gas_limit_lt: None,
        }
    }

    pub fn assert_filter(self, filter: Filter) -> Self {
        Self {
            transactions: self.transactions,
            filtered: self.filtered,
            filter: Some(filter),
            gas_limit_lt: self.gas_limit_lt,
        }
    }

    pub fn assert_gas_limit_lt(self, gas_limit: u64) -> Self {
        Self {
            transactions: self.transactions,
            filtered: self.filtered,
            filter: self.filter,
            gas_limit_lt: Some(gas_limit),
        }
    }
}

pub struct MockTxPool {
    response_queue: Arc<Mutex<VecDeque<MockTxPoolResponse>>>,
}

impl MockTxPool {
    pub fn push_response(&self, response: MockTxPoolResponse) {
        let response_queue = self.response_queue.clone();
        let mut response_queue = response_queue.lock().expect("Mutex poisoned");
        response_queue.push_back(response);
    }
}

impl MockTransactionsSource {
    pub fn new() -> (Self, MockTxPool) {
        let response_queue = Arc::new(Mutex::new(VecDeque::new()));
        (
            Self {
                response_queue: response_queue.clone(),
            },
            MockTxPool { response_queue },
        )
    }
}

impl TransactionsSource for MockTransactionsSource {
    fn get_executable_transactions(
        &mut self,
        gas_limit: u64,
        tx_count_limit: u16,
        _block_transaction_size_limit: u32,
        filter: Filter,
    ) -> TransactionSourceExecutableTransactions {
        loop {
            let mut response_queue = self.response_queue.lock().expect("Mutex poisoned");
            if let Some(response) = response_queue.pop_front() {
                assert!(response.transactions.len() <= tx_count_limit as usize);
                if let Some(expected_filter) = &response.filter {
                    assert_eq!(expected_filter, &filter);
                }
                if let Some(expected_gas_limit) = &response.gas_limit_lt {
                    assert!(
                        expected_gas_limit >= &gas_limit,
                        "Expected gas limit to be less than or equal to {}, but got {}",
                        expected_gas_limit,
                        gas_limit,
                    );
                }
                return TransactionSourceExecutableTransactions {
                    transactions: response.transactions,
                    filtered: response.filtered,
                    filter: response.filter.unwrap_or(filter),
                };
            } else {
                continue;
            }
        }
    }

    fn get_new_transactions_notifier(&mut self) -> tokio::sync::Notify {
        // This is a mock implementation, so we return a dummy Notify instance
        tokio::sync::Notify::new()
    }
}

fn into_checked_txs(txs: &[&Transaction]) -> Vec<CheckedTransaction> {
    txs.iter()
        .map(|&tx| {
            tx.clone()
                .into_checked_basic(0u32.into(), &ConsensusParameters::default())
                .unwrap()
                .into()
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct MockPreconfirmationSender;

impl PreconfirmationSenderPort for MockPreconfirmationSender {
    fn send(
        &self,
        _preconfirmations: Vec<
            fuel_core_types::services::preconfirmation::Preconfirmation,
        >,
    ) -> impl Future<Output = ()> + Send {
        futures::future::ready(())
    }

    fn try_send(&self, preconfirmations: Vec<Preconfirmation>) -> Vec<Preconfirmation> {
        preconfirmations
    }
}
