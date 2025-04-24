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
}
