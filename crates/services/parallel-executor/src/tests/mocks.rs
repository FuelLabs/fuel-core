use std::collections::HashSet;

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
    pub block_transaction_size_limit: u32,
    pub filter: Filter,
}

pub struct MockTransactionsSource {
    pub request_sender: MockTransactionsSourcesRequestSender,
}

pub type MockTransactionsSourcesRequestReceiver = std::sync::mpsc::Receiver<(
    PoolRequestParams,
    std::sync::mpsc::Sender<(Vec<CheckedTransaction>, TransactionFiltered, Filter)>,
)>;

pub type MockTransactionsSourcesRequestSender = std::sync::mpsc::Sender<(
    PoolRequestParams,
    std::sync::mpsc::Sender<(Vec<CheckedTransaction>, TransactionFiltered, Filter)>,
)>;

pub struct MockTxPool(MockTransactionsSourcesRequestReceiver);

impl MockTxPool {
    pub fn waiting_for_request_to_tx_pool(&self) -> Consumer {
        Consumer::receive(self)
    }
}

impl MockTransactionsSource {
    pub fn new() -> (Self, MockTxPool) {
        let (
            get_executable_transactions_results_sender,
            get_executable_transactions_results_receiver,
        ) = std::sync::mpsc::channel();
        (
            Self {
                request_sender: get_executable_transactions_results_sender,
            },
            MockTxPool(get_executable_transactions_results_receiver),
        )
    }
}

impl TransactionsSource for MockTransactionsSource {
    fn get_executable_transactions(
        &mut self,
        gas_limit: u64,
        tx_count_limit: u16,
        block_transaction_size_limit: u32,
        filter: Filter,
    ) -> (Vec<CheckedTransaction>, TransactionFiltered, Filter) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.request_sender
            .send((
                PoolRequestParams {
                    gas_limit,
                    tx_count_limit,
                    block_transaction_size_limit,
                    filter,
                },
                tx,
            ))
            .expect("Failed to send request");
        rx.recv().expect("Failed to receive response")
    }

    fn get_new_transactions_notifier(&mut self) -> tokio::sync::Notify {
        // This is a mock implementation, so we return a dummy Notify instance
        tokio::sync::Notify::new()
    }
}

pub struct Consumer {
    pool_request_params: PoolRequestParams,
    response_sender:
        std::sync::mpsc::Sender<(Vec<CheckedTransaction>, TransactionFiltered, Filter)>,
}

impl Consumer {
    fn receive(receiver: &MockTxPool) -> Self {
        let (pool_request_params, response_sender) = receiver.0.recv().unwrap();

        Self {
            pool_request_params,
            response_sender,
        }
    }

    pub fn assert_filter(&self, filter: &Filter) -> &Self {
        assert_eq!(&self.pool_request_params.filter, filter);
        self
    }

    pub fn assert_gas_limit_lt(&self, gas_limit: u64) -> &Self {
        assert!(
            self.pool_request_params.gas_limit < gas_limit,
            "Expected gas limit to be less than {}, but got {}",
            gas_limit,
            self.pool_request_params.gas_limit
        );
        self
    }

    pub fn respond_with(
        &self,
        txs: &[&Transaction],
        filtered: TransactionFiltered,
    ) -> &Self {
        let txs = into_checked_txs(txs);

        self.response_sender
            .send((
                txs,
                filtered,
                Filter {
                    excluded_contract_ids: HashSet::default(),
                },
            ))
            .unwrap();
        self
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
