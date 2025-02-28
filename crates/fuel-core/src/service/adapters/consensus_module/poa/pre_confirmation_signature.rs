use fuel_core_poa::{
    ports::GetTime,
    pre_confirmation_signature_service::{
        error::{
            Error as PoaError,
            Result as PoAResult,
        },
        parent_signature::ParentSignature,
        trigger::KeyRotationTrigger,
        tx_receiver::TxReceiver,
    },
};
use fuel_core_types::{
    fuel_crypto,
    fuel_tx::TxId,
    services::p2p::PreconfirmationStatus,
    signer::SignMode,
    tai64::Tai64,
};
use std::time::Duration;

#[cfg(test)]
mod trigger;

#[cfg(test)]
mod tx_receiver;

pub struct TimeBasedTrigger<Time> {
    time: Time,
    next_rotation: Tai64,
    rotation_interval: Duration,
}

impl<Time: GetTime> TimeBasedTrigger<Time> {
    pub fn new(time: Time, rotation_interval: Duration) -> Self {
        let now = time.now();
        let _next_rotation = now.0.saturating_add(rotation_interval.as_secs());
        let next_rotation = Tai64(_next_rotation);
        Self {
            time,
            next_rotation,
            rotation_interval,
        }
    }

    fn get_next_rotation(&self) -> PoAResult<Tai64> {
        Ok(self.next_rotation)
    }

    fn set_next_rotation(&mut self, next_rotation: Tai64) -> PoAResult<()> {
        self.next_rotation = next_rotation;
        Ok(())
    }
}

fn duration_between(t1: Tai64, t2: Tai64) -> Duration {
    let diff = t2.0.saturating_sub(t1.0);
    Duration::from_secs(diff)
}

impl<Time: GetTime + Send> KeyRotationTrigger for TimeBasedTrigger<Time> {
    async fn next_rotation(&mut self) -> PoAResult<()> {
        // create a future that will resolve at the next rotation time
        // if that future resolves, update the next rotation time
        let next_rotation = self.get_next_rotation()?;
        let time_to_wait = duration_between(self.time.now(), next_rotation);
        tokio::time::sleep(time_to_wait).await;
        tracing::debug!(
            "next rotation triggered after waiting {:?}",
            time_to_wait.as_secs()
        );
        let new_next_rotation = self
            .next_rotation
            .0
            .saturating_add(self.rotation_interval.as_secs());
        self.set_next_rotation(Tai64(new_next_rotation))?;
        tracing::debug!("next rotation set to {:?}", self.next_rotation);
        Ok(())
    }
}

pub struct FuelParentSigner<T> {
    mode: SignMode,
    _phantom: std::marker::PhantomData<T>,
}

pub struct FuelParentSignature<T> {
    signature: fuel_core_types::fuel_vm::Signature,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> FuelParentSignature<T> {
    pub fn signature(&self) -> fuel_core_types::fuel_vm::Signature {
        self.signature
    }
}

impl<T> From<fuel_core_types::fuel_vm::Signature> for FuelParentSignature<T> {
    fn from(signature: fuel_core_types::fuel_vm::Signature) -> Self {
        Self {
            signature,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> ParentSignature<T> for FuelParentSigner<T>
where
    T: Send + Sync,
    T: Into<fuel_crypto::Message>,
{
    type SignedData = FuelParentSignature<T>;

    async fn sign(&self, data: T) -> PoAResult<Self::SignedData> {
        let message = data.into();
        let signature = self.mode.sign_message(message).await.map_err(|e| {
            PoaError::ParentSignature(format!("Failed to sign message: {}", e))
        })?;
        Ok(signature.into())
    }
}

pub struct MPSCTxReceiver<T> {
    receiver: tokio::sync::mpsc::Receiver<T>,
}

impl TxReceiver for MPSCTxReceiver<Vec<(TxId, PreconfirmationStatus)>> {
    type Txs = Vec<(TxId, PreconfirmationStatus)>;

    async fn receive(&mut self) -> PoAResult<Self::Txs> {
        self.receiver
            .recv()
            .await
            .ok_or(PoaError::TxReceiver(format!(
                "Failed to receive transaction, channel closed"
            )))
    }
}
