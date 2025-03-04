#![allow(non_snake_case)]

use super::*;
use crate::pre_confirmation_signature_service::error::Error;
use fuel_core_services::StateWatcher;
use fuel_core_types::fuel_tx::Transaction;
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::sync::Notify;

use fuel_core_types::fuel_types::BlockHeight;

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Success { height: BlockHeight },
    Fail { height: BlockHeight },
    SqueezedOut { reason: String },
}

struct FakeTxReceiver {
    recv: tokio::sync::mpsc::Receiver<Vec<(Transaction, Status)>>,
}

impl TxReceiver for FakeTxReceiver {
    type Txs = Vec<(Transaction, Status)>;

    async fn receive(&mut self) -> Result<Vec<(Transaction, Status)>> {
        let txs = self.recv.recv().await;
        match txs {
            Some(txs) => Ok(txs),
            None => Err(Error::TxReceiver("No txs received".into())),
        }
    }
}

struct FakeBroadcast {
    delegation_key_sender: tokio::sync::mpsc::Sender<FakeSignedData<FakeSigningKey>>,
    tx_sender: tokio::sync::mpsc::Sender<FakeSignedData<Vec<(Transaction, Status)>>>,
}

impl Broadcast for FakeBroadcast {
    type PreConfirmations = Vec<(Transaction, Status)>;
    type ParentSignature = FakeSignedData<FakeSigningKey>;
    type DelegateKey = FakeSigningKey;

    async fn broadcast_txs(
        &mut self,
        txs: Signed<Self::DelegateKey, Self::PreConfirmations>,
    ) -> Result<()> {
        self.tx_sender.send(txs).await.map_err(|error| {
            Error::Broadcast(format!("Could not send {:?} over channel", error))
        })?;
        Ok(())
    }

    async fn broadcast_delegate_key(
        &mut self,
        delegate_key: Self::ParentSignature,
    ) -> Result<()> {
        self.delegation_key_sender
            .send(delegate_key)
            .await
            .map_err(|error| {
                Error::Broadcast(format!("Could not send {:?} over channel", error))
            })?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct FakeParentSignature<T> {
    dummy_signature: String,
    _phantom: std::marker::PhantomData<T>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct FakeSignedData<T> {
    data: T,
    dummy_signature: String,
}

impl<T: Send + Sync> ParentSignature<T> for FakeParentSignature<T> {
    type SignedData = FakeSignedData<T>;
    async fn sign(&self, data: T) -> Result<Self::SignedData> {
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

impl SigningKey for FakeSigningKey {
    type Signature<T>
        = FakeSignedData<T>
    where
        T: Send + Clone;

    fn sign<T: Send + Clone>(&self, data: T) -> Result<Self::Signature<T>> {
        Ok(FakeSignedData {
            data,
            dummy_signature: self.inner.clone(),
        })
    }
}

struct FakeTrigger {
    pub inner: Arc<Notify>,
}

impl KeyRotationTrigger for FakeTrigger {
    async fn next_rotation(&mut self) -> Result<()> {
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
    pub broadcast_delegation_key_handle:
        tokio::sync::mpsc::Receiver<FakeSignedData<FakeSigningKey>>,
    pub broadcast_tx_handle:
        tokio::sync::mpsc::Receiver<FakeSignedData<Vec<(Transaction, Status)>>>,
    pub tx_sender_handle: tokio::sync::mpsc::Sender<Vec<(Transaction, Status)>>,
}

struct TaskBuilder {
    current_delegate_key: Option<String>,
    parent_signature: Option<FakeParentSignature<FakeSigningKey>>,
    key_generator: Option<FakeKeyGenerator>,
}

type TestTask = PreConfirmationSignatureTask<
    FakeTxReceiver,
    FakeBroadcast,
    FakeParentSignature<FakeSigningKey>,
    FakeKeyGenerator,
    FakeSigningKey,
    FakeTrigger,
>;
impl TaskBuilder {
    pub fn new() -> Self {
        Self {
            current_delegate_key: None,
            parent_signature: None,
            key_generator: None,
        }
    }

    pub fn build_with_handles(self) -> (TestTask, TestImplHandles) {
        let (key_rotation_trigger, trigger_handle) = FakeTrigger::new();
        let current_delegate_key = self.get_current_delegate_key();
        let key_generator = self.get_key_generator();
        let parent_signature = self.get_parent_signature();
        let (broadcast, broadcast_delegation_key_handle, broadcast_tx_handle) =
            self.get_broadcast();
        let (tx_receiver, tx_sender_handle) = self.get_tx_receiver();
        let task = PreConfirmationSignatureTask {
            tx_receiver,
            broadcast,
            parent_signature,
            key_generator,
            current_delegate_key,
            key_rotation_trigger,
        };
        let handles = TestImplHandles {
            trigger_handle,
            broadcast_delegation_key_handle,
            broadcast_tx_handle,
            tx_sender_handle,
        };
        (task, handles)
    }

    fn get_current_delegate_key(&self) -> FakeSigningKey {
        let raw = self
            .current_delegate_key
            .clone()
            .unwrap_or("first key".to_string());
        FakeSigningKey::from(raw)
    }

    fn get_key_generator(&self) -> FakeKeyGenerator {
        self.key_generator.clone().unwrap_or_else(|| {
            self.key_generator.clone().unwrap_or(FakeKeyGenerator {
                key: "dummy key".into(),
            })
        })
    }

    fn get_parent_signature(&self) -> FakeParentSignature<FakeSigningKey> {
        self.parent_signature.clone().unwrap_or({
            FakeParentSignature {
                _phantom: std::marker::PhantomData,
                dummy_signature: "dummy signature".into(),
            }
        })
    }

    #[allow(clippy::type_complexity)]
    fn get_broadcast(
        &self,
    ) -> (
        FakeBroadcast,
        tokio::sync::mpsc::Receiver<FakeSignedData<FakeSigningKey>>,
        tokio::sync::mpsc::Receiver<FakeSignedData<Vec<(Transaction, Status)>>>,
    ) {
        let (delegation_key_sender, delegation_key_receiver) =
            tokio::sync::mpsc::channel(10);
        let (tx_sender, tx_receiver) = tokio::sync::mpsc::channel(10);
        let broadcast = FakeBroadcast {
            delegation_key_sender,
            tx_sender,
        };
        (broadcast, delegation_key_receiver, tx_receiver)
    }

    fn get_tx_receiver(
        &self,
    ) -> (
        FakeTxReceiver,
        tokio::sync::mpsc::Sender<Vec<(Transaction, Status)>>,
    ) {
        let (tx_sender, tx_receiver) = tokio::sync::mpsc::channel(1);
        let tx_receiver = FakeTxReceiver { recv: tx_receiver };
        (tx_receiver, tx_sender)
    }

    pub fn with_current_delegate_key<T: Into<String>>(
        mut self,
        current_delegate_key: T,
    ) -> Self {
        self.current_delegate_key = Some(current_delegate_key.into());
        self
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
            _phantom: std::marker::PhantomData,
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
    tokio::task::spawn(async move { task.run(&mut state_watcher).await });
    handles.trigger_handle.notify_one();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // then
    let actual = handles.broadcast_delegation_key_handle.try_recv().unwrap();
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
async fn run__received_tx_will_be_broadcast_with_current_delegate_key_signature() {
    // given
    let current_delegate_key = "foobar delegate key";
    let (mut task, mut handles) = TaskBuilder::new()
        .with_current_delegate_key(current_delegate_key)
        .build_with_handles();
    let mut state_watcher = StateWatcher::started();

    // when
    let txs = vec![
        (
            Transaction::default(),
            Status::Success {
                height: BlockHeight::from(1),
            },
        ),
        (
            Transaction::default(),
            Status::Fail {
                height: BlockHeight::from(2),
            },
        ),
    ];
    tokio::task::spawn(async move {
        let _ = task.run(&mut state_watcher).await;
    });

    handles
        .tx_sender_handle
        .send(txs.clone())
        .await
        .expect("Failed to send txs");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // then
    let actual = handles.broadcast_tx_handle.try_recv().unwrap();
    let dummy_signature = current_delegate_key.to_string();
    let expected = FakeSignedData {
        data: txs,
        dummy_signature,
    };
    assert_eq!(expected, actual);
}
