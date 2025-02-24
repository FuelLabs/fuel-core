#![allow(non_snake_case)]
// TODO: Remove

use super::*;
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

struct FakeBroadcast;

#[async_trait::async_trait]
impl Broadcast for FakeBroadcast {
    async fn _broadcast<T: Send>(&self, _data: T) -> Result<()> {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct FakeParentSignature {
    dummy_signature: String,
    notify: Arc<Notify>,
}

struct FakeSignedDat<T> {
    data: T,
    dummy_signature: String,
}

impl ParentSignature for FakeParentSignature {
    type SignedData<T> = FakeSignedDat<T>;
    fn sign<T>(&self, data: T) -> Result<Self::SignedData<T>> {
        self.notify.notify_one();
        Ok(FakeSignedDat {
            data,
            dummy_signature: self.dummy_signature.clone(),
        })
    }
}

struct FakeKeyGenerator {
    pub inner: Arc<Notify>,
    pub key: FakeSigningKey,
}

impl FakeKeyGenerator {
    pub fn new(key: FakeSigningKey) -> (Self, Arc<Notify>) {
        let notify = Notify::new();
        let inner = Arc::new(notify);
        let handle = inner.clone();
        let key_generator = Self { inner, key };
        (key_generator, handle)
    }
}

#[async_trait::async_trait]
impl KeyGenerator for FakeKeyGenerator {
    type Key = FakeSigningKey;
    async fn generate(&mut self) -> Result<Self::Key> {
        self.inner.notify_one();
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
    pub gen_handle: Arc<Notify>,
    pub parent_signature_handle: Arc<Notify>,
}

struct TaskBuilder {
    tx_receiver: Option<FakeTxReceiver>,
    broadcast: Option<FakeBroadcast>,
    parent_signature: Option<(FakeParentSignature, Arc<Notify>)>,
    key_generator: Option<FakeKeyGenerator>,
    current_delegate_key: Option<FakeSigningKey>,
    key_rotation_trigger: Option<FakeTrigger>,
}

impl TaskBuilder {
    pub fn new() -> Self {
        Self {
            tx_receiver: None,
            broadcast: None,
            parent_signature: None,
            key_generator: None,
            current_delegate_key: None,
            key_rotation_trigger: None,
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
        let next_delegate_key = "second key".into();
        let (key_generator, gen_handle) = FakeKeyGenerator::new(next_delegate_key);
        let (parent_signature, parent_signature_handle) = self.get_parant_signature();
        let task = PreConfirmationTask {
            tx_receiver: FakeTxReceiver,
            broadcast: FakeBroadcast,
            parent_signature,
            key_generator,
            current_delegate_key,
            key_rotation_trigger,
        };
        let handles = TestImplHandles {
            trigger_handle,
            gen_handle,
            parent_signature_handle,
        };
        (task, handles)
    }

    fn get_parant_signature(&self) -> (FakeParentSignature, Arc<Notify>) {
        self.parent_signature.clone().unwrap_or_else(|| {
            let notify = Notify::new();
            let inner = Arc::new(notify);
            let handle = inner.clone();
            let task = FakeParentSignature {
                dummy_signature: "dummy signature".into(),
                notify: inner,
            };
            (task, handle)
        })
    }

    pub fn with_dummy_parent_signature<T: Into<String>>(
        mut self,
        dummy_signature: T,
    ) -> Self {
        let notify = Notify::new();
        let inner = Arc::new(notify);
        let handle = inner.clone();
        let parent_signature = FakeParentSignature {
            dummy_signature: dummy_signature.into(),
            notify: inner,
        };
        self.parent_signature = Some((parent_signature, handle));
        self
    }
}

#[tokio::test]
async fn run__will_generate_key_on_trigger() {
    // let _ = tracing_subscriber::fmt()
    //     .with_max_level(tracing::Level::DEBUG)
    //     .try_init();

    // given
    let (mut task, handles) = TaskBuilder::new().build_with_handles();
    let mut state_watcher = StateWatcher::started();

    // when
    let _ = tokio::task::spawn(async move { task.run(&mut state_watcher).await });
    handles.trigger_handle.notify_one();

    // then
    let timeout = Duration::from_millis(100);
    let _ = tokio::time::timeout(timeout, handles.gen_handle.notified())
        .await
        .expect("Should get hit and not time out");
}

#[tokio::test]
async fn run__generated_key_will_get_signed_by_parent() {
    // given
    let (mut task, handles) = TaskBuilder::new()
        .with_dummy_parent_signature("dummy signature")
        .build_with_handles();
    let mut state_watcher = StateWatcher::started();

    // when
    let _ = tokio::task::spawn(async move { task.run(&mut state_watcher).await });
    handles.trigger_handle.notify_one();

    // then
    let timeout = Duration::from_millis(100);
    let _ = tokio::time::timeout(timeout, handles.parent_signature_handle.notified())
        .await
        .expect("Should get hit and not time out");
}


