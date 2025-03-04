use fuel_core_executor::ports::NewTxWaiterPort;
use fuel_core_types::services::executor::WaitNewTransactionsResult;

pub struct NewTxWaiter;

impl NewTxWaiterPort for NewTxWaiter {
    async fn wait_for_new_transactions(&self) -> WaitNewTransactionsResult {
        WaitNewTransactionsResult::Timeout
    }
}
