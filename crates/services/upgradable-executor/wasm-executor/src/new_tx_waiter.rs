use fuel_core_executor::{
    executor::WaitNewTransactionsResult,
    ports::NewTxWaiterPort,
};

pub struct NewTxWaiter;

impl NewTxWaiterPort for NewTxWaiter {
    async fn wait_for_new_transactions(&mut self) -> WaitNewTransactionsResult {
        WaitNewTransactionsResult::Timeout
    }
}
