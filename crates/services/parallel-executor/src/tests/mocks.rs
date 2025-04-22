use std::collections::HashSet;

use fuel_core_types::{
    fuel_tx::TxId,
    fuel_types::ChainId,
    fuel_vm::checked_transaction::CheckedTransaction,
};
use fuel_core_upgradable_executor::native_executor::ports::MaybeCheckedTransaction;

use crate::ports::TransactionsSource;

pub struct MockStorage;

pub struct MockRelayer;

#[derive(Debug)]
pub struct PoolRequestParams {
    gas_limit: u64,
    tx_count_limit: u16,
    block_transaction_size_limit: u32,
    excluded_contract_ids: HashSet<fuel_core_types::fuel_tx::ContractId>,
}

pub struct MockTxPool {
    pub txs: Vec<(MaybeCheckedTransaction, PriorityPosition)>,
    pub get_executable_transactions_results_sender:
        std::sync::mpsc::Sender<(PoolRequestParams, Vec<TxId>)>,
}

impl MockTxPool {
    pub fn new() -> (
        Self,
        std::sync::mpsc::Receiver<(PoolRequestParams, Vec<TxId>)>,
    ) {
        let (
            get_executable_transactions_results_sender,
            get_executable_transactions_results_receiver,
        ) = std::sync::mpsc::channel();
        (
            Self {
                txs: vec![],
                get_executable_transactions_results_sender,
            },
            get_executable_transactions_results_receiver,
        )
    }
}

pub type PriorityPosition = u32;

impl MockTxPool {
    pub fn register_transactions(
        &mut self,
        txs: Vec<(CheckedTransaction, PriorityPosition)>,
    ) {
        self.txs.extend(txs.iter().map(|(tx, priority)| {
            (
                MaybeCheckedTransaction::CheckedTransaction(tx.clone(), 0),
                *priority,
            )
        }));
    }
}

impl TransactionsSource for MockTxPool {
    fn get_executable_transactions(
        &mut self,
        gas_limit: u64,
        tx_count_limit: u16,
        block_transaction_size_limit: u32,
        excluded_contract_ids: HashSet<fuel_core_types::fuel_tx::ContractId>,
    ) -> Vec<MaybeCheckedTransaction> {
        let res: Vec<MaybeCheckedTransaction> = std::mem::take(&mut self.txs)
            .into_iter()
            .map(|(tx, _)| tx)
            .collect();
        self.get_executable_transactions_results_sender
            .send((
                PoolRequestParams {
                    gas_limit,
                    tx_count_limit,
                    block_transaction_size_limit,
                    excluded_contract_ids,
                },
                res.iter().map(|tx| tx.id(&ChainId::default())).collect(),
            ))
            .expect("Failed to send request");
        res
    }
}
