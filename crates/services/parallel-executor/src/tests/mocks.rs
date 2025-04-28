use fuel_core_types::{
    fuel_tx::{
        ConsensusParameters,
        Transaction,
    },
    fuel_vm::checked_transaction::IntoChecked,
};
use fuel_core_upgradable_executor::native_executor::ports::MaybeCheckedTransaction;

use crate::ports::{
    Filter,
    TransactionFiltered,
    TransactionsSource,
};

pub struct MockRelayer;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PoolRequestParams {
    pub gas_limit: u64,
    pub tx_count_limit: u16,
    pub block_transaction_size_limit: u32,
    pub filter: Filter,
}

pub struct MockTxPool {
    pub get_executable_transactions_results_sender: GetExecutableTransactionsSender,
}

pub type GetExecutableTransactionsSender = std::sync::mpsc::Sender<(
    PoolRequestParams,
    std::sync::mpsc::Sender<(Vec<MaybeCheckedTransaction>, TransactionFiltered)>,
)>;

pub type GetExecutableTransactionsReceiver = std::sync::mpsc::Receiver<(
    PoolRequestParams,
    std::sync::mpsc::Sender<(Vec<MaybeCheckedTransaction>, TransactionFiltered)>,
)>;

impl MockTxPool {
    pub fn new() -> (Self, GetExecutableTransactionsReceiver) {
        let (
            get_executable_transactions_results_sender,
            get_executable_transactions_results_receiver,
        ) = std::sync::mpsc::channel();
        (
            Self {
                get_executable_transactions_results_sender,
            },
            get_executable_transactions_results_receiver,
        )
    }
}

impl TransactionsSource for MockTxPool {
    fn get_executable_transactions(
        &mut self,
        gas_limit: u64,
        tx_count_limit: u16,
        block_transaction_size_limit: u32,
        filter: Filter,
    ) -> (Vec<MaybeCheckedTransaction>, TransactionFiltered) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.get_executable_transactions_results_sender
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
        std::sync::mpsc::Sender<(Vec<MaybeCheckedTransaction>, TransactionFiltered)>,
}

impl Consumer {
    pub fn receive(receiver: &GetExecutableTransactionsReceiver) -> Self {
        let (pool_request_params, response_sender) = receiver.recv().unwrap();

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

        self.response_sender.send((txs, filtered)).unwrap();
        self
    }
}

fn into_checked_txs(txs: &[&Transaction]) -> Vec<MaybeCheckedTransaction> {
    txs.iter()
        .map(|&tx| {
            MaybeCheckedTransaction::CheckedTransaction(
                tx.clone()
                    .into_checked_basic(0u32.into(), &ConsensusParameters::default())
                    .unwrap()
                    .into(),
                0,
            )
        })
        .collect()
}
