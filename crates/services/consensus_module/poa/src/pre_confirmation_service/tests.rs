#![allow(non_snake_case)]

use super::*;
use fuel_core_services::StateWatcher;
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::sync::Notify;

struct FakeTxReceiver;

#[async_trait::async_trait]
impl TxReceiver for FakeTxReceiver {
    async fn receive(&mut self) -> Result<Vec<Transaction>> {
        let _: () = std::future::pending().await;
        todo!()
    }
}

struct FakeBroadcast;

#[async_trait::async_trait]
impl Broadcast for FakeBroadcast {
    async fn _broadcast<T: Send>(&self, _data: T) -> Result<()> {
        todo!()
    }
}

struct FakeParentSignature;
impl ParentSignature for FakeParentSignature {
    fn _sign<T>(&self, _data: T) -> Sealed<T> {
        todo!()
    }
}

struct FakeKeyGenerator {
    pub inner: Arc<Notify>,
}

impl FakeKeyGenerator {
    pub fn new() -> (Self, Arc<Notify>) {
        let notify = Notify::new();
        let inner = Arc::new(notify);
        let handle = inner.clone();
        let key_generator = Self { inner };
        (key_generator, handle)
    }
}

#[async_trait::async_trait]
impl KeyGenerator for FakeKeyGenerator {
    type Key = FakeSigningKey;
    async fn generate(&mut self) -> Result<Self::Key> {
        self.inner.notify_one();
        Ok(FakeSigningKey)
    }
}

struct FakeSigningKey;

impl SigningKey for FakeSigningKey {}

struct FakeTrigger {
    pub inner: Arc<Notify>,
}

#[async_trait::async_trait]
impl KeyRotationTrigger for FakeTrigger {
    async fn next_rotation(&self) -> Result<()> {
        self.inner.notified().await;
        Ok(())
    }
}

impl FakeTrigger {
    pub fn new() -> (Self, Arc<Notify>) {
        let notify = Notify::new();
        let inner = Arc::new(notify);
        let handle = inner.clone();
        let trigger = Self { inner };
        (trigger, handle)
    }
}

#[tokio::test]
async fn run__will_generate_key_on_trigger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    // given
    let (key_rotation_trigger, trigger_handle) = FakeTrigger::new();
    let (key_generator, gen_handle) = FakeKeyGenerator::new();
    let mut task = PreConfirmationTask {
        tx_receiver: FakeTxReceiver,
        _broadcast: FakeBroadcast,
        _parent_signature: FakeParentSignature,
        key_generator,
        current_delegate_key: FakeSigningKey,
        key_rotation_trigger,
    };
    let mut state_watcher = StateWatcher::started();

    // when
    let _ = tokio::task::spawn(async move { task.run(&mut state_watcher).await });
    trigger_handle.notify_one();

    // then
    let timeout = Duration::from_millis(100);
    let _ = tokio::time::timeout(timeout, gen_handle.notified())
        .await
        .expect("Should get hit and not time out");
}
