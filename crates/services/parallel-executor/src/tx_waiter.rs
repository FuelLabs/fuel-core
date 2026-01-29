use fuel_core_executor::{
    executor::WaitNewTransactionsResult,
    ports::NewTxWaiterPort,
};

#[derive(Debug, Clone)]
pub struct NoWaitTxs;

impl NewTxWaiterPort for NoWaitTxs {
    fn wait_for_new_transactions(
        &mut self,
    ) -> impl Future<Output = fuel_core_executor::executor::WaitNewTransactionsResult> + Send
    {
        futures::future::ready(WaitNewTransactionsResult::Timeout)
    }
}
