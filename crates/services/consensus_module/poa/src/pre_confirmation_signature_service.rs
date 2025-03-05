use error::Result;
use fuel_core_services::{
    EmptyShared,
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::{
    services::p2p::{
        DelegatePreConfirmationKey,
        Sealed,
    },
    tai64::Tai64,
};
use serde::Serialize;
use tokio::time::Interval;

use crate::pre_confirmation_signature_service::{
    broadcast::Broadcast,
    key_generator::{
        ExpiringKey,
        KeyGenerator,
    },
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

pub struct PreConfirmationSignatureTask<
    TxReceiver,
    Broadcast,
    Parent,
    KeyGenerator,
    DelegateKey,
    KeyRotationTrigger,
> where
    Parent: ParentSignature,
    DelegateKey: SigningKey,
{
    tx_receiver: TxReceiver,
    broadcast: Broadcast,
    parent_signature: Parent,
    key_generator: KeyGenerator,
    current_delegate_key: ExpiringKey<DelegateKey>,
    sealed_delegate_message:
        Sealed<DelegatePreConfirmationKey<DelegateKey::PublicKey>, Parent::Signature>,
    key_rotation_trigger: KeyRotationTrigger,
    echo_delegation_trigger: Interval,
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
    pub async fn _run(&mut self) -> Result<()> {
        tracing::debug!("Running pre-confirmation task");
        tokio::select! {
            res = self.tx_receiver.receive() => {
                tracing::debug!("Received transactions");
                let pre_confirmations = res?;
                let signature = self.current_delegate_key.sign(&pre_confirmations)?;
                let expiration = self.current_delegate_key.expiration();

                let result = self.broadcast.broadcast_preconfirmations(pre_confirmations, signature, expiration).await;
                if let Err(err) = result {
                    tracing::error!("Failed to broadcast pre-confirmations: {:?}", err);
                }
            }
            res = self.key_rotation_trigger.next_rotation() => {
                tracing::debug!("Key rotation triggered");
                let expiration = res?;

                let (new_delegate_key, sealed) = create_delegate_key(
                    &mut self.key_generator,
                    &self.parent_signature,
                    expiration,
                ).await?;

                self.current_delegate_key = new_delegate_key;
                self.sealed_delegate_message = sealed.clone();

                if let Err(err) = self.broadcast.broadcast_delegate_key(sealed.entity, sealed.signature).await {
                    tracing::error!("Failed to broadcast newly generated delegate key: {:?}", err);
                }
            }
            _ = self.echo_delegation_trigger.tick() => {
                tracing::debug!("Echo delegation trigger");
                let sealed = self.sealed_delegate_message.clone();

                if let Err(err) = self.broadcast.broadcast_delegate_key(sealed.entity, sealed.signature).await {
                    tracing::error!("Failed to re-broadcast delegate key: {:?}", err);
                }
            }
        }
        Ok(())
    }
}

async fn create_delegate_key<Gen, DelegateKey, Parent>(
    key_generator: &mut Gen,
    parent_signature: &Parent,
    expiration: Tai64,
) -> Result<(
    ExpiringKey<DelegateKey>,
    Sealed<DelegatePreConfirmationKey<DelegateKey::PublicKey>, Parent::Signature>,
)>
where
    Gen: KeyGenerator<Key = DelegateKey>,
    DelegateKey: SigningKey,
    Parent: ParentSignature,
{
    let new_delegate_key = key_generator.generate(expiration).await;
    let public_key = new_delegate_key.public_key();

    let message = DelegatePreConfirmationKey {
        public_key,
        expiration,
    };

    const MAX_ATTEMPTS: usize = 5;

    let mut attempts = 0usize;
    let signed_key = {
        loop {
            attempts = attempts.saturating_add(1);
            match parent_signature.sign(&message).await {
                Ok(signature) => break signature,
                Err(err) => {
                    tracing::error!("Failed to sign delegate key: {:?}", err);
                }
            }

            if attempts >= MAX_ATTEMPTS {
                tracing::error!(
                    "Failed to sign delegate key after {} attempts",
                    MAX_ATTEMPTS
                );
                return Err(error::Error::ParentSignature(
                    "Failed to sign delegate key".to_string(),
                ));
            }
        }
    };

    let sealed = Sealed {
        entity: message,
        signature: signed_key,
    };

    Ok((new_delegate_key, sealed))
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
                match res {
                    Ok(_) => {
                        TaskNextAction::Continue
                    }
                    Err(err) => {
                        tracing::error!("Error running pre-confirmation task: {:?}. Stopping the task", err);
                        TaskNextAction::Stop
                    }
                }
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}
