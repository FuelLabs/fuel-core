use error::Result;
use fuel_core_services::{
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};

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

pub type Signed<K, T> = <K as SigningKey>::Signature<T>;

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

impl<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>
    PreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>
where
    TxRcv: TxReceiver,
    Brdcst: Broadcast<
        DelegateKey = DelegateKey,
        ParentSignature = Parent::SignedData,
        PreConfirmations: From<TxRcv::Txs>,
    >,
    Parent: ParentSignature<DelegateKey>,
    Gen: KeyGenerator<Key = DelegateKey>,
    DelegateKey: SigningKey,
    Trigger: KeyRotationTrigger,
{
    pub async fn _run(&mut self) -> Result<()> {
        tracing::debug!("Running pre-confirmation task");
        tokio::select! {
            res = self.tx_receiver.receive() => {
                tracing::debug!("Received transactions");
                let txs = res?;
                let pre_confirmations = Brdcst::PreConfirmations::from(txs);
                let signed = self.current_delegate_key.sign(pre_confirmations)?;
                self.broadcast.broadcast_txs(signed).await?;
            }
            _ = self.key_rotation_trigger.next_rotation() => {
                tracing::debug!("Key rotation triggered");
                let new_delegate_key = self.key_generator.generate().await?;
                let signed_key = self.parent_signature.sign(new_delegate_key.clone()).await?;
                self.broadcast.broadcast_delegate_key(signed_key).await?;
                self.current_delegate_key = new_delegate_key;
            }
        }
        Ok(())
    }
}

impl<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger> RunnableTask
    for PreConfirmationSignatureTask<TxRcv, Brdcst, Parent, Gen, DelegateKey, Trigger>
where
    TxRcv: TxReceiver,
    Brdcst: Broadcast<
        DelegateKey = DelegateKey,
        ParentSignature = Parent::SignedData,
        PreConfirmations: From<TxRcv::Txs>,
    >,
    Parent: ParentSignature<DelegateKey>,
    Gen: KeyGenerator<Key = DelegateKey>,
    DelegateKey: SigningKey,
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
        Ok(())
    }
}
