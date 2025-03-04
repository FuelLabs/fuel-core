use error::Result;
use fuel_core_services::{
    EmptyShared,
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::{
    services::p2p::DelegatePreConfirmationKey,
    tai64::Tai64,
};
use serde::Serialize;

use crate::pre_confirmation_signature_service::{
    broadcast::Broadcast,
    key_generator::KeyGenerator,
    parent_signature::ParentSignature,
    signing_key::SigningKey,
    trigger::KeyRotationTrigger,
    tx_receiver::TxReceiver,
};

pub mod broadcast;
pub mod error;
pub mod key_generator;
pub mod parent_signature;
pub mod signing_key;
pub mod trigger;
pub mod tx_receiver;

#[cfg(test)]
pub mod tests;

// TODO(#2739): Remove when integrated
// link: https://github.com/FuelLabs/fuel-core/issues/2739
#[allow(dead_code)]
pub struct PreConfirmationSignatureTask<
    TxReceiver,
    Broadcast,
    ParentSignature,
    KeyGenerator,
    DelegateKey,
    KeyRotationTrigger,
> {
    tx_receiver: TxReceiver,
    broadcast: Broadcast,
    parent_signature: ParentSignature,
    key_generator: KeyGenerator,
    current_delegate_key: DelegateKey,
    key_rotation_trigger: KeyRotationTrigger,
}

#[async_trait::async_trait]
impl<Preconfirmations, Parent, DelegateKey, TxRcv, Brdcst, Gen, Trigger> RunnableService
    for PreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>
where
    TxRcv: TxReceiver<Txs = Preconfirmations>,
    Brdcst: Broadcast<
        DelegateKey = DelegateKey,
        ParentKey = Parent,
        Preconfirmations = Preconfirmations,
    >,
    Gen: KeyGenerator<Key = DelegateKey>,
    Trigger: KeyRotationTrigger,
    DelegateKey: SigningKey,
    Parent: ParentSignature,
    Preconfirmations: serde::Serialize + Send,
{
    const NAME: &'static str = "PreConfirmationSignatureTask";
    type SharedData = EmptyShared;
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        EmptyShared
    }

    async fn into_task(
        self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

impl<Preconfirmations, Parent, DelegateKey, TxRcv, Brdcst, Gen, Trigger>
    PreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>
where
    TxRcv: TxReceiver<Txs = Preconfirmations>,
    Brdcst: Broadcast<
        DelegateKey = DelegateKey,
        ParentKey = Parent,
        Preconfirmations = Preconfirmations,
    >,
    Gen: KeyGenerator<Key = DelegateKey>,
    Trigger: KeyRotationTrigger,
    DelegateKey: SigningKey,
    Parent: ParentSignature,
    Preconfirmations: serde::Serialize + Send,
{
    // TODO: Handle errors in a proper way
    pub async fn _run(&mut self) -> Result<()> {
        tracing::debug!("Running pre-confirmation task");
        tokio::select! {
            res = self.tx_receiver.receive() => {
                tracing::debug!("Received transactions");
                let pre_confirmations = res?;
                let signature = self.current_delegate_key.sign(&pre_confirmations)?;
                self.broadcast.broadcast_preconfirmations(pre_confirmations, signature).await?;
            }
            _ = self.key_rotation_trigger.next_rotation() => {
                tracing::debug!("Key rotation triggered");
                let new_delegate_key = self.key_generator.generate().await?;
                let public_key = new_delegate_key.public_key();

                let message = DelegatePreConfirmationKey {
                    public_key,
                    expiration: Tai64::now(),
                };

                let signed_key = self.parent_signature.sign(&message).await?;
                self.broadcast.broadcast_delegate_key(message, signed_key).await?;
                self.current_delegate_key = new_delegate_key;
            }
        }
        Ok(())
    }
}

impl<Preconfirmations, Parent, DelegateKey, TxRcv, Brdcst, Gen, Trigger> RunnableTask
    for PreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>
where
    TxRcv: TxReceiver<Txs = Preconfirmations>,
    Brdcst: Broadcast<
        DelegateKey = DelegateKey,
        ParentKey = Parent,
        Preconfirmations = Preconfirmations,
    >,
    Gen: KeyGenerator<Key = DelegateKey>,
    Trigger: KeyRotationTrigger,
    DelegateKey: SigningKey,
    Parent: ParentSignature,
    Preconfirmations: serde::Serialize + Send,
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
        Ok(())
    }
}
