use error::Result;
use fuel_core_services::{
    EmptyShared,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
    try_or_continue,
    try_or_stop,
};
use fuel_core_types::{
    services::{
        p2p::{
            DelegatePreConfirmationKey,
            Sealed,
        },
        preconfirmation::{
            Preconfirmation,
            Preconfirmations,
        },
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

pub struct UninitializedPreConfirmationSignatureTask<
    TxReceiver,
    Broadcast,
    Parent,
    KeyGenerator,
    KeyRotationTrigger,
> where
    Parent: ParentSignature,
{
    tx_receiver: TxReceiver,
    broadcast: Broadcast,
    parent_signature: Parent,
    key_generator: KeyGenerator,
    key_rotation_trigger: KeyRotationTrigger,
    echo_delegation_trigger: Interval,
}

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
    nonce: u64,
    key_rotation_trigger: KeyRotationTrigger,
    echo_delegation_trigger: Interval,
}

#[async_trait::async_trait]
impl<Parent, DelegateKey, TxRcv, Brdcst, Gen, Trigger> RunnableService
    for UninitializedPreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, Trigger>
where
    TxRcv: TxReceiver<Txs = Vec<Preconfirmation>>,
    Brdcst: Broadcast<
            DelegateKey = DelegateKey,
            ParentKey = Parent,
            Preconfirmations = Preconfirmations,
        >,
    Gen: KeyGenerator<Key = DelegateKey>,
    Trigger: KeyRotationTrigger,
    DelegateKey: SigningKey,
    Parent: ParentSignature,
{
    const NAME: &'static str = "PreconfirmationSignatureTask";
    type SharedData = EmptyShared;
    type Task =
        PreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        EmptyShared
    }

    async fn into_task(
        self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let Self {
            tx_receiver,
            mut broadcast,
            parent_signature,
            mut key_generator,
            mut key_rotation_trigger,
            echo_delegation_trigger,
        } = self;

        // The first key rotation is triggered immediately
        let expiration = key_rotation_trigger.next_rotation().await?;
        let (new_delegate_key, sealed) =
            create_delegate_key(&mut key_generator, &parent_signature, expiration)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

        let mut nonce = 0;
        if let Err(e) = broadcast
            .broadcast_delegate_key(
                sealed.entity.clone(),
                nonce,
                sealed.signature.clone(),
            )
            .await
        {
            tracing::error!("Failed to broadcast delegate key: {:?}", e);
        }
        nonce = nonce.saturating_add(1);
        let initialized_task = PreConfirmationSignatureTask {
            tx_receiver,
            broadcast,
            parent_signature,
            key_generator,
            current_delegate_key: new_delegate_key,
            sealed_delegate_message: sealed,
            nonce,
            key_rotation_trigger,
            echo_delegation_trigger,
        };
        Ok(initialized_task)
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

impl<Parent, DelegateKey, TxRcv, Brdcst, Gen, Trigger> RunnableTask
    for PreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>
where
    TxRcv: TxReceiver<Txs = Vec<Preconfirmation>>,
    Brdcst: Broadcast<
            DelegateKey = DelegateKey,
            ParentKey = Parent,
            Preconfirmations = Preconfirmations,
        >,
    Gen: KeyGenerator<Key = DelegateKey>,
    Trigger: KeyRotationTrigger,
    DelegateKey: SigningKey,
    Parent: ParentSignature,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tracing::debug!("Running pre-confirmation task");
        tokio::select! {
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
            res = self.tx_receiver.receive() => {
                tracing::debug!("Received transactions");
                let preconfirmations = try_or_stop!(res);
                if preconfirmations.is_empty() {
                    tracing::debug!("No pre-confirmations received");
                } else {
                    let expiration = self.current_delegate_key.expiration();
                    let pre_confirmations = Preconfirmations {
                        expiration,
                        preconfirmations,
                    };
                    let signature = try_or_stop!(self.current_delegate_key.sign(&pre_confirmations));
                    try_or_continue!(
                        self.broadcast.broadcast_preconfirmations(pre_confirmations, signature).await,
                        |err| tracing::error!("Failed to broadcast pre-confirmations: {:?}", err)
                    );
                }
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
                self.nonce = 0;

                try_or_continue!(
                    self.broadcast.broadcast_delegate_key(sealed.entity, self.nonce, sealed.signature).await,
                    |err| tracing::error!("Failed to broadcast newly generated delegate key: {:?}", err)
                );
                self.nonce = self.nonce.saturating_add(1);
                TaskNextAction::Continue
            }
            _ = self.echo_delegation_trigger.tick() => {
                tracing::debug!("Echo delegation trigger");
                let sealed = self.sealed_delegate_message.clone();

                try_or_continue!(
                    self.broadcast.broadcast_delegate_key(sealed.entity, self.nonce, sealed.signature).await,
                    |err| tracing::error!("Failed to re-broadcast delegate key: {:?}", err)
                );
                self.nonce = self.nonce.saturating_add(1);
                TaskNextAction::Continue
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub type Service<TxRcv, Brdcst, Parent, Gen, Trigger> = ServiceRunner<
    UninitializedPreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, Trigger>,
>;

pub fn new_service<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>(
    config: config::Config,
    preconfirmation_receiver: TxRcv,
    p2p_adapter: Brdcst,
    parent_signature: Parent,
    key_generator: Gen,
    key_rotation_trigger: Trigger,
) -> anyhow::Result<Service<TxRcv, Brdcst, Parent, Gen, Trigger>>
where
    TxRcv: TxReceiver<Txs = Vec<Preconfirmation>>,
    Brdcst: Broadcast<
            DelegateKey = DelegateKey,
            ParentKey = Parent,
            Preconfirmations = Preconfirmations,
        >,
    Gen: KeyGenerator<Key = DelegateKey>,
    Trigger: KeyRotationTrigger,
    DelegateKey: SigningKey,
    Parent: ParentSignature,
{
    Ok(Service::new(UninitializedPreConfirmationSignatureTask {
        tx_receiver: preconfirmation_receiver,
        broadcast: p2p_adapter,
        parent_signature,
        key_generator,
        key_rotation_trigger,
        echo_delegation_trigger: tokio::time::interval(config.echo_delegation_interval),
    }))
}
