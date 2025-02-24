use crate::pre_confirmation_service::trigger::KeyRotationTrigger;
use error::Result;
use fuel_core_services::{
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::fuel_tx::Transaction;

pub mod error;
pub mod trigger;

#[cfg(test)]
pub mod tests;

pub struct PreConfirmationTask<
    TxReceiver,
    Broadcast,
    ParentSignature,
    KeyGenerator,
    Key,
    KeyRotationTrigger,
> {
    tx_receiver: TxReceiver,
    _broadcast: Broadcast,
    _parent_signature: ParentSignature,
    key_generator: KeyGenerator,
    current_delegate_key: Key,
    key_rotation_trigger: KeyRotationTrigger,
}

#[async_trait::async_trait]
pub trait TxReceiver: Send {
    async fn receive(&mut self) -> Result<Vec<Transaction>>;
}

#[async_trait::async_trait]
pub trait Broadcast: Send {
    async fn _broadcast<T: Send>(&self, data: T) -> Result<()>;
}

#[allow(dead_code)]
pub struct Sealed<T> {
    _phantom: std::marker::PhantomData<T>,
}

pub trait ParentSignature: Send {
    fn _sign<T>(&self, data: T) -> Sealed<T>;
}

#[async_trait::async_trait]
pub trait KeyGenerator: Send {
    type Key: SigningKey;
    async fn generate(&mut self) -> Result<Self::Key>;
}

pub trait SigningKey: Send {}

impl<TxRcv, Brdcst, Parent, Gen, Key, Trigger>
    PreConfirmationTask<TxRcv, Brdcst, Parent, Gen, Key, Trigger>
where
    TxRcv: TxReceiver,
    Brdcst: Broadcast,
    Parent: ParentSignature,
    Gen: KeyGenerator<Key = Key>,
    Key: SigningKey,
    Trigger: KeyRotationTrigger,
{
    pub async fn _run(&mut self) -> Result<()> {
        tracing::debug!("Running pre-confirmation task");
        tokio::select! {
            _txs = self.tx_receiver.receive() => {
                tracing::debug!("Received transactions");
                todo!();
            }
            _ = self.key_rotation_trigger.next_rotation() => {
                tracing::debug!("Key rotation triggered");
                self.current_delegate_key = self.key_generator.generate().await?;
            }
        }
        Ok(())
    }
}

impl<TxRcv, Brdcst, Parent, Gen, Key, Trigger> RunnableTask
    for PreConfirmationTask<TxRcv, Brdcst, Parent, Gen, Key, Trigger>
where
    TxRcv: TxReceiver,
    Brdcst: Broadcast,
    Parent: ParentSignature,
    Gen: KeyGenerator<Key = Key>,
    Key: SigningKey,
    Trigger: KeyRotationTrigger,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
            res = self._run() => {
                TaskNextAction::always_continue(res)
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        todo!()
    }
}
