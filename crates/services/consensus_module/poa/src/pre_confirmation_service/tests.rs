#![allow(non_snake_case)]
// TODO: Remove

use super::*;
use crate::pre_confirmation_service::error::Error;
use fuel_core_services::StateWatcher;
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::sync::Notify;
use tracing_subscriber as _;

struct FakeTxReceiver;

#[async_trait::async_trait]
impl TxReceiver for FakeTxReceiver {
    async fn _receive(&mut self) -> Result<Vec<Transaction>> {
        let _: () = std::future::pending().await;
        todo!()
    }
}

struct FakeBroadcast {
    sender: Option<tokio::sync::oneshot::Sender<FakeSignedData<FakeSigningKey>>>,
}

#[async_trait::async_trait]
impl Broadcast<FakeSignedData<FakeSigningKey>> for FakeBroadcast {
    async fn broadcast(&mut self, data: FakeSignedData<FakeSigningKey>) -> Result<()> {
        match self.sender.take() {
            Some(sender) => {
                let _ = sender.send(data).map_err(|data| {
                    Error::BroadcastError(format!(
                        "Could not sent {:?} over oneshot channel",
                        data
                    ))
                })?;
            }
            None => {
                tracing::warn!("Broadcast channel already closed");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct FakeParentSignature {
    dummy_signature: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct FakeSignedData<T> {
    data: T,
    dummy_signature: String,
}

impl ParentSignature for FakeParentSignature {
    type SignedData<T>
        = FakeSignedData<T>
    where
        T: Send;
    fn sign<T: Send>(&self, data: T) -> Result<Self::SignedData<T>> {
        Ok(FakeSignedData {
            data,
            dummy_signature: self.dummy_signature.clone(),
        })
    }
}

#[derive(Debug, Clone)]
struct FakeKeyGenerator {
    pub key: FakeSigningKey,
}

impl FakeKeyGenerator {
    pub fn new(key: FakeSigningKey) -> Self {
        Self { key }
    }
}

#[async_trait::async_trait]
impl KeyGenerator for FakeKeyGenerator {
    type Key = FakeSigningKey;
    async fn generate(&mut self) -> Result<Self::Key> {
        Ok(self.key.clone())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct FakeSigningKey {
    inner: String,
}

impl<T: Into<String>> From<T> for FakeSigningKey {
    fn from(value: T) -> Self {
        Self {
            inner: value.into(),
        }
    }
}

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

struct TestImplHandles {
    pub trigger_handle: Arc<Notify>,
    pub broadcast_handle: tokio::sync::oneshot::Receiver<FakeSignedData<FakeSigningKey>>,
}

struct TaskBuilder {
    parent_signature: Option<FakeParentSignature>,
    key_generator: Option<FakeKeyGenerator>,
}

impl TaskBuilder {
    pub fn new() -> Self {
        Self {
            parent_signature: None,
            key_generator: None,
        }
    }

    pub fn build_with_handles(
        self,
    ) -> (
        PreConfirmationTask<
            FakeTxReceiver,
            FakeBroadcast,
            FakeParentSignature,
            FakeKeyGenerator,
            FakeSigningKey,
            FakeTrigger,
        >,
        TestImplHandles,
    ) {
        let (key_rotation_trigger, trigger_handle) = FakeTrigger::new();
        let current_delegate_key = "first key".into();
        let key_generator = self.get_key_generator();
        let parent_signature = self.get_parant_signature();
        let (broadcast, broadcast_handle) = self.get_broadcast();
        let task = PreConfirmationTask {
            tx_receiver: FakeTxReceiver,
            broadcast,
            parent_signature,
            key_generator,
            current_delegate_key,
            key_rotation_trigger,
        };
        let handles = TestImplHandles {
            trigger_handle,
            broadcast_handle,
        };
        (task, handles)
    }

    fn get_key_generator(&self) -> FakeKeyGenerator {
        self.key_generator.clone().unwrap_or_else(|| {
            self.key_generator.clone().unwrap_or(FakeKeyGenerator {
                key: "dummy key".into(),
            })
        })
    }

    fn get_parant_signature(&self) -> FakeParentSignature {
        self.parent_signature.clone().unwrap_or({
            FakeParentSignature {
                dummy_signature: "dummy signature".into(),
            }
        })
    }

    fn get_broadcast(
        &self,
    ) -> (
        FakeBroadcast,
        tokio::sync::oneshot::Receiver<FakeSignedData<FakeSigningKey>>,
    ) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let broadcast = FakeBroadcast {
            sender: Some(sender),
        };
        (broadcast, receiver)
    }

    pub fn with_generated_key<T: Into<String>>(mut self, generated_key: T) -> Self {
        let key_raw: String = generated_key.into();
        let key_generator = FakeKeyGenerator::new(key_raw.into());
        self.key_generator = Some(key_generator);
        self
    }

    pub fn with_dummy_parent_signature<T: Into<String>>(
        mut self,
        dummy_signature: T,
    ) -> Self {
        let parent_signature = FakeParentSignature {
            dummy_signature: dummy_signature.into(),
        };
        self.parent_signature = Some(parent_signature);
        self
    }
}

#[tokio::test]
async fn run__key_rotation_trigger_will_broadcast_generated_key_with_correct_signature() {
    // given
    let generated_key = "some generated key";
    let dummy_signature = "dummy signature";
    let (mut task, mut handles) = TaskBuilder::new()
        .with_generated_key(generated_key)
        .with_dummy_parent_signature(dummy_signature)
        .build_with_handles();

    // when
    let mut state_watcher = StateWatcher::started();
    let _ = tokio::task::spawn(async move { task.run(&mut state_watcher).await });
    handles.trigger_handle.notify_one();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // then
    let actual = handles.broadcast_handle.try_recv().unwrap();
    let expected = FakeSignedData {
        data: generated_key.into(),
        dummy_signature: dummy_signature.into(),
    };
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__key_rotation_updates_current_key() {
    // given
    let generated_key = "another generated key";
    let (mut task, handles) = TaskBuilder::new()
        .with_generated_key(generated_key)
        .build_with_handles();

    // when
    let mut state_watcher = StateWatcher::started();
    let fut_current_key = tokio::task::spawn(async move {
        let _ = task.run(&mut state_watcher).await;
        task.current_delegate_key
    });
    handles.trigger_handle.notify_one();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // then
    let actual = fut_current_key.await.unwrap();
    let expected = FakeSigningKey::from(generated_key);
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__received_tx_will_be_broadcast_with_current_delegate_key() {
    todo!()
}
