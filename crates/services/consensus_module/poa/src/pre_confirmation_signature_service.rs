use std::{
    fmt::Debug,
    sync::Arc,
};

use error::Result;
use fuel_core_services::{
    try_or_continue,
    try_or_stop,
    EmptyShared,
    RunnableService,
    RunnableTask,
    ServiceRunner,
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
pub mod config;
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
    parent_signature: Arc<Parent>,
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
    Preconfirmations: serde::Serialize + Send + Debug,
{
    const NAME: &'static str = "PreconfirmationSignatureTask";
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

async fn create_delegate_key<Gen, DelegateKey, Parent>(
    key_generator: &mut Gen,
    parent_signature: &Arc<Parent>,
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
    Preconfirmations: serde::Serialize + Send + Debug,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tracing::debug!("Running pre-confirmation task");
        tokio::select! {
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
            res = self.tx_receiver.receive() => {
                tracing::debug!("Received transactions");
                let pre_confirmations = try_or_stop!(res);
                let signature = try_or_stop!(self.current_delegate_key.sign(&pre_confirmations));
                let expiration = self.current_delegate_key.expiration();
                try_or_continue!(
                    self.broadcast.broadcast_preconfirmations(pre_confirmations, signature, expiration).await,
                    |err| tracing::error!("Failed to broadcast pre-confirmations: {:?}", err)
                );
                TaskNextAction::Continue
            }
            res = self.key_rotation_trigger.next_rotation() => {
                tracing::debug!("Key rotation triggered");
                let expiration = try_or_stop!(res);

                let (new_delegate_key, sealed) = try_or_stop!(create_delegate_key(
                    &mut self.key_generator,
                    &self.parent_signature,
                    expiration,
                ).await);

                self.current_delegate_key = new_delegate_key;
                self.sealed_delegate_message = sealed.clone();

                try_or_continue!(
                    self.broadcast.broadcast_delegate_key(sealed.entity, sealed.signature).await,
                    |err| tracing::error!("Failed to broadcast newly generated delegate key: {:?}", err)
                );
                TaskNextAction::Continue
            }
            _ = self.echo_delegation_trigger.tick() => {
                tracing::debug!("Echo delegation trigger");
                let sealed = self.sealed_delegate_message.clone();

                try_or_continue!(
                    self.broadcast.broadcast_delegate_key(sealed.entity, sealed.signature).await,
                    |err| tracing::error!("Failed to re-broadcast delegate key: {:?}", err)
                );
                TaskNextAction::Continue
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub type Service<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger> = ServiceRunner<
    PreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>,
>;

pub fn new_service<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger, Preconfirmations>(
    config: config::Config,
    preconfirmation_receiver: TxRcv,
    mut p2p_adapter: Brdcst,
    parent_signature: Arc<Parent>,
    mut key_generator: Gen,
    key_rotation_trigger: Trigger,
) -> anyhow::Result<Service<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>>
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
    Preconfirmations: serde::Serialize + Send + Debug,
{
    // It's ok to wait because the first rotation is expected to be immediate
    let expiration = Tai64(
        Tai64::now()
            .0
            .saturating_add(config.key_expiration_interval.as_secs()),
    );
    let (new_delegate_key, sealed) = futures::executor::block_on(create_delegate_key(
        &mut key_generator,
        &parent_signature,
        expiration,
    ))
    .map_err(|e| anyhow::anyhow!(e))?;

    if let Err(e) = futures::executor::block_on(
        p2p_adapter
            .broadcast_delegate_key(sealed.entity.clone(), sealed.signature.clone()),
    ) {
        tracing::error!("Failed to broadcast delegate key: {:?}", e);
    }

    Ok(Service::new(PreConfirmationSignatureTask {
        tx_receiver: preconfirmation_receiver,
        broadcast: p2p_adapter,
        parent_signature,
        key_generator,
        current_delegate_key: new_delegate_key,
        sealed_delegate_message: sealed,
        key_rotation_trigger,
        echo_delegation_trigger: tokio::time::interval(config.echo_delegation_interval),
    }))
}
