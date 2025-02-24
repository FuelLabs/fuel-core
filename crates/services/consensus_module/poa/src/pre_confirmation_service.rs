// TODO: Remove this
#![allow(dead_code)]

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
    broadcast: Broadcast,
    parent_signature: ParentSignature,
    key_generator: KeyGenerator,
    current_delegate_key: Key,
    key_rotation_trigger: KeyRotationTrigger,
}

#[async_trait::async_trait]
pub trait TxReceiver: Send {
    async fn _receive(&mut self) -> Result<Vec<Transaction>>;
}

#[async_trait::async_trait]
pub trait Broadcast<T>: Send {
    async fn broadcast(&mut self, data: T) -> Result<()>;
}

pub trait ParentSignature: Send {
    type SignedData<T>: Send
    where
        T: Send;
    fn sign<T: Send>(&self, data: T) -> Result<Self::SignedData<T>>;
}

#[async_trait::async_trait]
pub trait KeyGenerator: Send {
    type Key: SigningKey;
    async fn generate(&mut self) -> Result<Self::Key>;
}

pub trait SigningKey: Clone + Send {}

impl<TxRcv, Brdcst, Parent, Gen, Key, Trigger>
    PreConfirmationTask<TxRcv, Brdcst, Parent, Gen, Key, Trigger>
where
    TxRcv: TxReceiver,
    Brdcst: Broadcast<Parent::SignedData<Key>>,
    Parent: ParentSignature,
    Gen: KeyGenerator<Key = Key>,
    Key: SigningKey,
    Trigger: KeyRotationTrigger,
{
    pub async fn _run(&mut self) -> Result<()> {
        tracing::debug!("Running pre-confirmation task");
        tokio::select! {
            // _txs = self.tx_receiver.receive() => {
            //     tracing::debug!("Received transactions");
            //     todo!();
            // }
            _ = self.key_rotation_trigger.next_rotation() => {
                tracing::debug!("Key rotation triggered");
                let new_delegate_key = self.key_generator.generate().await?;
                tracing::debug!("Generated new delegate key");
                let signed_key = self.parent_signature.sign(new_delegate_key.clone())?;
                tracing::debug!("Signed new delegate key");
                self.broadcast.broadcast(signed_key).await?;

            }
        }
        Ok(())
    }
}

impl<TxRcv, Brdcst, Parent, Gen, Key, Trigger> RunnableTask
    for PreConfirmationTask<TxRcv, Brdcst, Parent, Gen, Key, Trigger>
where
    TxRcv: TxReceiver,
    Brdcst: Broadcast<Parent::SignedData<Key>>,
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
