#![allow(non_snake_case)]

use super::*;
use crate::pre_confirmation_signature_service::{
    broadcast::{
        PublicKey,
        Signature,
    },
    error::Error,
};
use fuel_core_services::StateWatcher;
use fuel_core_types::{
    fuel_tx::{
        Bytes32,
        TxId,
    },
    fuel_types::BlockHeight,
    services::preconfirmation::PreconfirmationStatus,
};
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Status {
    Success { height: BlockHeight },
    Fail { height: BlockHeight },
    SqueezedOut { reason: String },
}

pub struct FakeTxReceiver {
    recv: tokio::sync::mpsc::Receiver<Vec<Preconfirmation>>,
}

impl TxReceiver for FakeTxReceiver {
    type Txs = Vec<Preconfirmation>;

    async fn receive(&mut self) -> Result<Vec<Preconfirmation>> {
        let txs = self.recv.recv().await;
        match txs {
            Some(txs) => Ok(txs),
            None => Err(Error::TxReceiver("No txs received".into())),
        }
    }
}

pub struct FakeBroadcast {
    delegation_key_sender: tokio::sync::mpsc::Sender<
        FakeParentSignedData<DelegatePreConfirmationKey<Bytes32>>,
    >,
    tx_sender: tokio::sync::mpsc::Sender<FakeDelegateSignedData<Preconfirmations>>,
}

impl Broadcast for FakeBroadcast {
    type DelegateKey = FakeSigningKey;
    type ParentKey = FakeParentSignature;
    type Preconfirmations = Preconfirmations;

    async fn broadcast_preconfirmations(
        &mut self,
        message: Self::Preconfirmations,
        signature: Signature<Self>,
    ) -> Result<()> {
        let txs = FakeDelegateSignedData {
            data: message,
            dummy_signature: signature,
        };
        self.tx_sender.send(txs).await.map_err(|error| {
            Error::Broadcast(format!("Could not send {:?} over channel", error))
        })?;
        Ok(())
    }

    async fn broadcast_delegate_key(
        &mut self,
        data: DelegatePreConfirmationKey<PublicKey<Self>>,
        _: u64,
        signature: <Self::ParentKey as ParentSignature>::Signature,
    ) -> Result<()> {
        let delegate_key = FakeParentSignedData {
            data,
            dummy_signature: signature,
        };

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
pub struct FakeParentSignature {
    dummy_signature: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FakeDelegateSignedData<T> {
    data: T,
    dummy_signature: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FakeParentSignedData<T> {
    data: T,
    dummy_signature: String,
}

impl ParentSignature for FakeParentSignature {
    type Signature = String;

    async fn sign<T>(&self, _: &T) -> Result<Self::Signature>
    where
        T: Serialize + Send + Sync,
    {
        Ok(self.dummy_signature.clone())
    }
}

#[derive(Debug, Clone)]
pub struct FakeKeyGenerator {
    pub key: FakeSigningKey,
}

impl FakeKeyGenerator {
    pub fn new(key: FakeSigningKey) -> Self {
        Self { key }
    }
}

impl KeyGenerator for FakeKeyGenerator {
    type Key = FakeSigningKey;
    async fn generate(&mut self, expiration: Tai64) -> ExpiringKey<Self::Key> {
        ExpiringKey::new(self.key.clone(), expiration)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FakeSigningKey {
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
    type Signature = String;
    type PublicKey = Bytes32;

    fn public_key(&self) -> Self::PublicKey {
        Bytes32::zeroed()
    }

    fn sign<T>(&self, _: &T) -> Result<Self::Signature>
    where
        T: Serialize,
    {
        Ok(self.inner.clone())
    }
}

pub struct FakeTrigger {
    pub inner: tokio::sync::mpsc::Receiver<Tai64>,
}

impl KeyRotationTrigger for FakeTrigger {
    async fn next_rotation(&mut self) -> Result<Tai64> {
        let expiration = self.inner.recv().await.unwrap();
        Ok(expiration)
    }
}

impl FakeTrigger {
    pub fn new() -> (Self, tokio::sync::mpsc::Sender<Tai64>) {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        let trigger = Self { inner: receiver };
        (trigger, sender)
    }
}

pub struct TestImplHandles {
    pub trigger_handle: tokio::sync::mpsc::Sender<Tai64>,
    pub broadcast_delegation_key_handle: tokio::sync::mpsc::Receiver<
        FakeParentSignedData<DelegatePreConfirmationKey<Bytes32>>,
    >,
    pub broadcast_tx_handle:
        tokio::sync::mpsc::Receiver<FakeDelegateSignedData<Preconfirmations>>,
    pub tx_sender_handle: tokio::sync::mpsc::Sender<Vec<Preconfirmation>>,
}

#[derive(Default)]
pub struct TaskBuilder {
    current_delegate_key: Option<String>,
    current_delegate_key_expiration: Option<Tai64>,
    parent_signature: Option<FakeParentSignature>,
    key_generator: Option<FakeKeyGenerator>,
    period: Option<Duration>,
}

type TestTask = PreConfirmationSignatureTask<
    FakeTxReceiver,
    FakeBroadcast,
    FakeParentSignature,
    FakeKeyGenerator,
    FakeSigningKey,
    FakeTrigger,
>;
impl TaskBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_with_handles(self) -> (TestTask, TestImplHandles) {
        let (key_rotation_trigger, trigger_handle) = FakeTrigger::new();
        let current_delegate_key = self.get_current_delegate_key();
        let current_delegate_key_expiration = self.get_current_delegate_key_expiration();
        let current_delegate_key =
            ExpiringKey::new(current_delegate_key, current_delegate_key_expiration);
        let key_generator = self.get_key_generator();
        let parent_signature = self.get_parent_signature();
        let entity = DelegatePreConfirmationKey {
            public_key: Bytes32::zeroed(),
            expiration: Tai64::UNIX_EPOCH,
        };
        let signature = parent_signature.dummy_signature.clone();
        let period = self.period.unwrap_or(Duration::from_secs(60 * 60));
        let (broadcast, broadcast_delegation_key_handle, broadcast_tx_handle) =
            self.get_broadcast();
        let (tx_receiver, tx_sender_handle) = self.get_tx_receiver();
        let task = PreConfirmationSignatureTask {
            tx_receiver,
            broadcast,
            parent_signature,
            key_generator,
            current_delegate_key,
            sealed_delegate_message: Sealed { entity, signature },
            nonce: 0,
            key_rotation_trigger,
            echo_delegation_trigger: tokio::time::interval(period),
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

    fn get_current_delegate_key_expiration(&self) -> Tai64 {
        self.current_delegate_key_expiration
            .unwrap_or(Tai64::UNIX_EPOCH)
    }

    fn get_key_generator(&self) -> FakeKeyGenerator {
        self.key_generator.clone().unwrap_or_else(|| {
            self.key_generator.clone().unwrap_or(FakeKeyGenerator {
                key: "dummy key".into(),
            })
        })
    }

    fn get_parent_signature(&self) -> FakeParentSignature {
        self.parent_signature.clone().unwrap_or({
            FakeParentSignature {
                dummy_signature: "dummy signature".into(),
            }
        })
    }

    #[allow(clippy::type_complexity)]
    fn get_broadcast(
        &self,
    ) -> (
        FakeBroadcast,
        tokio::sync::mpsc::Receiver<
            FakeParentSignedData<DelegatePreConfirmationKey<Bytes32>>,
        >,
        tokio::sync::mpsc::Receiver<FakeDelegateSignedData<Preconfirmations>>,
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
        tokio::sync::mpsc::Sender<Vec<Preconfirmation>>,
    ) {
        let (tx_sender, tx_receiver) = tokio::sync::mpsc::channel(1);
        let tx_receiver = FakeTxReceiver { recv: tx_receiver };
        (tx_receiver, tx_sender)
    }

    pub fn with_current_delegate_key<T: Into<String>>(
        mut self,
        current_delegate_key: T,
        expiration: Tai64,
    ) -> Self {
        self.current_delegate_key = Some(current_delegate_key.into());
        self.current_delegate_key_expiration = Some(expiration);
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
            dummy_signature: dummy_signature.into(),
        };
        self.parent_signature = Some(parent_signature);
        self
    }

    pub fn with_echo_duration(mut self, period: Duration) -> Self {
        self.period = Some(period);
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
    let expiration_time = Tai64::from_unix(1_234_567);
    handles.trigger_handle.send(expiration_time).await.unwrap();

    // then
    let actual = handles
        .broadcast_delegation_key_handle
        .recv()
        .await
        .unwrap();
    let expected = FakeParentSignedData {
        data: DelegatePreConfirmationKey {
            public_key: Bytes32::zeroed(),
            expiration: expiration_time,
        },
        dummy_signature: dummy_signature.into(),
    };

    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__will_rebroadcast_generated_key_with_correct_signature_after_1_second() {
    // Given
    let period = Duration::from_secs(1);
    let generated_key = "some generated key";
    let dummy_signature = "dummy signature";
    let (mut task, mut handles) = TaskBuilder::new()
        .with_generated_key(generated_key)
        .with_dummy_parent_signature(dummy_signature)
        .with_echo_duration(period)
        .build_with_handles();

    // When
    let mut state_watcher = StateWatcher::started();
    tokio::task::spawn(async move {
        // Handle the key rotation
        let _ = task.run(&mut state_watcher).await;
        // Handle the echo delegation trigger
        let _ = task.run(&mut state_watcher).await;
    });
    let expiration_time = Tai64::from_unix(1_234_567);
    handles.trigger_handle.send(expiration_time).await.unwrap();

    // Then
    let actual_1 = handles
        .broadcast_delegation_key_handle
        .recv()
        .await
        .unwrap();
    let actual_2 = handles
        .broadcast_delegation_key_handle
        .recv()
        .await
        .unwrap();

    let expected = FakeParentSignedData {
        data: DelegatePreConfirmationKey {
            public_key: Bytes32::zeroed(),
            expiration: expiration_time,
        },
        dummy_signature: dummy_signature.into(),
    };

    assert_eq!(actual_1.data.expiration, actual_2.data.expiration);
    assert_eq!(actual_1.data.public_key, actual_2.data.public_key);
    assert_eq!(actual_1.dummy_signature, actual_2.dummy_signature);
    assert_eq!(expected, actual_1);
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
    let expiration_time = Tai64::from_unix(1_234_567);
    handles.trigger_handle.send(expiration_time).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // then
    let actual = fut_current_key.await.unwrap().key;
    let expected = FakeSigningKey::from(generated_key);
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__received_tx_will_be_broadcast_with_current_delegate_key_signature() {
    // given
    let current_delegate_key = "foobar delegate key";
    let (mut task, mut handles) = TaskBuilder::new()
        .with_current_delegate_key(current_delegate_key, Tai64::UNIX_EPOCH)
        .build_with_handles();
    let mut state_watcher = StateWatcher::started();

    // when
    let txs = vec![
        Preconfirmation {
            tx_id: TxId::zeroed(),
            status: PreconfirmationStatus::SqueezedOut {
                reason: "foo".into(),
            },
        },
        Preconfirmation {
            tx_id: TxId::zeroed(),
            status: PreconfirmationStatus::SqueezedOut {
                reason: "bar".into(),
            },
        },
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
    let expected = FakeDelegateSignedData {
        data: Preconfirmations {
            expiration: Tai64::UNIX_EPOCH,
            preconfirmations: txs,
        },
        dummy_signature,
    };
    assert_eq!(expected, actual);
}
#[tokio::test]
async fn run__received_empty_tx_will_not_be_broadcasted() {
    // given
    let current_delegate_key = "foobar delegate key";
    let (mut task, mut handles) = TaskBuilder::new()
        .with_current_delegate_key(current_delegate_key, Tai64::UNIX_EPOCH)
        .build_with_handles();
    let mut state_watcher = StateWatcher::started();

    // when
    let empyt_txs = vec![];

    tokio::task::spawn(async move {
        // run it on repeat so the receiver isn't dropped
        loop {
            let _ = task.run(&mut state_watcher).await;
        }
    });

    handles
        .tx_sender_handle
        .send(empyt_txs.clone())
        .await
        .expect("Failed to send txs");

    // then
    let response = handles.broadcast_tx_handle.recv();
    let _ = tokio::time::timeout(Duration::from_millis(100), response)
        .await
        .expect_err("should_not_receive_response");
}
