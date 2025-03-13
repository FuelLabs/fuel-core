use crate::{
    config::Config,
    manager::TxStatusManager,
    ports::{
        P2PPreConfirmationGossipData,
        P2PPreConfirmationMessage,
        P2PSubscriptions,
    },
    subscriptions::Subscriptions,
    update_sender::TxStatusChange,
    TxStatusStream,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::{
    fuel_tx::{
        Bytes32,
        Bytes64,
        TxId,
    },
    services::{
        p2p::{
            DelegatePreConfirmationKey,
            DelegatePublicKey,
            GossipData,
            PreConfirmationMessage,
            ProtocolSignature,
            Sealed,
        },
        preconfirmation::{
            Preconfirmation,
            Preconfirmations,
        },
        txpool::TransactionStatus,
    },
};
use futures::StreamExt;
use std::future::Future;
use tokio::sync::{
    mpsc,
    oneshot,
};

enum ReadRequest {
    GetStatus {
        tx_id: TxId,
        sender: oneshot::Sender<Option<TransactionStatus>>,
    },
    Subscribe {
        tx_id: TxId,
        sender: oneshot::Sender<anyhow::Result<TxStatusStream>>,
    },
}

enum WriteRequest {
    UpdateStatus {
        tx_id: TxId,
        status: TransactionStatus,
    },
    NotifySkipped {
        tx_ids_and_reason: Vec<(Bytes32, String)>,
    },
}

#[derive(Clone)]
pub struct SharedData {
    read_requests_sender: mpsc::Sender<ReadRequest>,
    write_requests_sender: mpsc::UnboundedSender<WriteRequest>,
}

impl SharedData {
    pub async fn get_status(
        &self,
        tx_id: TxId,
    ) -> anyhow::Result<Option<TransactionStatus>> {
        let (sender, receiver) = oneshot::channel();
        let request = ReadRequest::GetStatus { tx_id, sender };
        self.read_requests_sender.send(request).await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn subscribe(&self, tx_id: TxId) -> anyhow::Result<TxStatusStream> {
        let (sender, receiver) = oneshot::channel();
        let request = ReadRequest::Subscribe { tx_id, sender };
        self.read_requests_sender.send(request).await?;
        receiver.await?
    }

    pub fn update_status(&self, tx_id: TxId, status: TransactionStatus) {
        let request = WriteRequest::UpdateStatus { tx_id, status };
        let _ = self.write_requests_sender.send(request);
    }

    pub fn notify_skipped(&self, tx_ids_and_reason: Vec<(Bytes32, String)>) {
        let request = WriteRequest::NotifySkipped { tx_ids_and_reason };
        let _ = self.write_requests_sender.send(request);
    }
}

pub struct Task<T> {
    manager: TxStatusManager,
    subscriptions: Subscriptions,
    read_requests_receiver: mpsc::Receiver<ReadRequest>,
    write_requests_receiver: mpsc::UnboundedReceiver<WriteRequest>,
    shared_data: SharedData,
    signature_verification: T,
    early_preconfirmations: Vec<Sealed<Preconfirmations, Bytes64>>,
}

/// Interface for signature verification of preconfirmations
pub trait SignatureVerification: Send {
    /// Adds a new delegate signature to verify the preconfirmations
    fn add_new_delegate(
        &mut self,
        sealed: &Sealed<DelegatePreConfirmationKey<DelegatePublicKey>, ProtocolSignature>,
    ) -> impl Future<Output = bool> + Send;

    /// Checks pre-confirmation signature
    fn check_preconfirmation_signature(
        &mut self,
        sealed: &Sealed<Preconfirmations, Bytes64>,
    ) -> impl Future<Output = bool> + Send;
}

impl<T: SignatureVerification> Task<T> {
    fn handle_verified_preconfirmation(
        &mut self,
        sealed: Sealed<Preconfirmations, Bytes64>,
    ) {
        tracing::debug!("Preconfirmation signature verified");
        let Sealed { entity, .. } = sealed;
        entity.preconfirmations.into_iter().for_each(
            |Preconfirmation { tx_id, status }| {
                let status: TransactionStatus = status.into();
                self.manager.status_update(tx_id, status);
            },
        );
    }

    async fn new_preconfirmations_from_p2p(
        &mut self,
        preconfirmations: P2PPreConfirmationMessage,
    ) {
        match preconfirmations {
            PreConfirmationMessage::Delegate(sealed) => {
                tracing::debug!(
                    "Received new delegate signature from peer: {:?}",
                    sealed.entity.public_key
                );
                let _ = self.signature_verification.add_new_delegate(&sealed).await;
                let drained = std::mem::take(&mut self.early_preconfirmations);
                for sealed in drained {
                    if self
                        .signature_verification
                        .check_preconfirmation_signature(&sealed)
                        .await
                    {
                        self.handle_verified_preconfirmation(sealed);
                    } else {
                        tracing::warn!("Preconfirmation signature verification failed for early preconfirmation, removing it");
                    }
                }
            }
            PreConfirmationMessage::Preconfirmations(sealed) => {
                tracing::debug!("Received new preconfirmations from peer");
                if self
                    .signature_verification
                    .check_preconfirmation_signature(&sealed)
                    .await
                {
                    self.handle_verified_preconfirmation(sealed);
                } else {
                    // TODO: Allow to retry later when a new delegate key is added
                    tracing::warn!("Preconfirmation signature verification failed, will try once more when a new delegate key is added");
                    self.early_preconfirmations.push(sealed);
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl<T: SignatureVerification> RunnableService for Task<T> {
    const NAME: &'static str = "TxStatusManagerTask";
    type SharedData = SharedData;
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_data.clone()
    }

    async fn into_task(
        self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

impl<T: SignatureVerification> RunnableTask for Task<T> {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }

            tx_status_from_p2p = self.subscriptions.new_tx_status.next() => {
                if let Some(GossipData { data, .. }) = tx_status_from_p2p {
                    if let Some(msg) = data {
                        self.new_preconfirmations_from_p2p(msg).await;
                    }
                    TaskNextAction::Continue
                } else {
                    TaskNextAction::Stop
                }
            }

            request = self.write_requests_receiver.recv() => {
                match request {
                    Some(WriteRequest::UpdateStatus { tx_id, status }) => {
                        self.manager.status_update(tx_id, status);
                        TaskNextAction::Continue
                    }
                    Some(WriteRequest::NotifySkipped { tx_ids_and_reason }) => {
                        self.manager.notify_skipped_txs(tx_ids_and_reason);
                        TaskNextAction::Continue
                    }
                    None => TaskNextAction::Stop,
                }
            }

            request = self.read_requests_receiver.recv() => {
                match request {
                    Some(ReadRequest::GetStatus { tx_id, sender }) => {
                        let status = self.manager.status(&tx_id);
                        let _ = sender.send(status.cloned());
                        TaskNextAction::Continue
                    }
                    Some(ReadRequest::Subscribe { tx_id, sender }) => {
                        let result = self.manager.tx_update_subscribe(tx_id);
                        let _ = sender.send(result);
                        TaskNextAction::Continue
                    }
                    None => TaskNextAction::Stop,
                }
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn new_service<P2P, Sign: SignatureVerification>(
    p2p: P2P,
    signature_verification: Sign,
    config: Config,
) -> ServiceRunner<Task<Sign>>
where
    P2P: P2PSubscriptions<GossipedStatuses = P2PPreConfirmationGossipData>,
{
    let tx_status_from_p2p_stream = p2p.gossiped_tx_statuses();
    let subscriptions = Subscriptions {
        new_tx_status: tx_status_from_p2p_stream,
    };

    let tx_status_sender =
        TxStatusChange::new(config.max_tx_update_subscriptions, config.subscription_ttl);
    let tx_status_manager =
        TxStatusManager::new(tx_status_sender, config.status_cache_ttl);

    let (read_requests_sender, read_requests_receiver) =
        mpsc::channel(config.max_tx_update_subscriptions);
    let (write_requests_sender, write_requests_receiver) = mpsc::unbounded_channel();

    let shared_data = SharedData {
        read_requests_sender,
        write_requests_sender,
    };

    ServiceRunner::new(Task {
        subscriptions,
        manager: tx_status_manager,
        read_requests_receiver,
        write_requests_receiver,
        shared_data,
        signature_verification,
        early_preconfirmations: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use super::*;
    use crate::{
        tests::FakeSignatureVerification,
        update_sender::{
            MpscChannel,
            UpdateSender,
        },
        TxStatusMessage,
    };
    use fuel_core_services::Service;
    use fuel_core_types::services::{
        p2p::{
            DelegatePreConfirmationKey,
            Tai64,
        },
        preconfirmation::{
            PreconfirmationStatus,
            Preconfirmations,
        },
    };
    use std::time::Duration;
    use tokio_stream::wrappers::ReceiverStream;

    const TTL: Duration = Duration::from_secs(4);

    struct Handles {
        pub pre_confirmation_updates: mpsc::Sender<P2PPreConfirmationGossipData>,
        pub update_sender: UpdateSender,
    }

    fn new_task_with_handles<T: SignatureVerification>(
        signature_verification: T,
    ) -> (Task<T>, Handles) {
        let (read_requests_sender, read_requests_receiver) = mpsc::channel(1);
        let (write_requests_sender, write_requests_receiver) = mpsc::unbounded_channel();
        let shared_data = SharedData {
            read_requests_sender,
            write_requests_sender,
        };
        let (sender, receiver) = mpsc::channel(1_000);
        let new_tx_status = Box::pin(ReceiverStream::new(receiver));
        let subscriptions = Subscriptions { new_tx_status };

        let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
        let updater_sender = tx_status_change.update_sender.clone();
        let tx_status_manager = TxStatusManager::new(tx_status_change, TTL);
        let task = Task {
            manager: tx_status_manager,
            subscriptions,
            read_requests_receiver,
            write_requests_receiver,
            shared_data,
            signature_verification,
            early_preconfirmations: Vec::new(),
        };
        let handles = Handles {
            pre_confirmation_updates: sender,
            update_sender: updater_sender,
        };
        (task, handles)
    }

    fn arbitrary_delegate_signatures_message() -> P2PPreConfirmationGossipData {
        let signature = ProtocolSignature::from_bytes([1u8; 64]);
        let delegate_key = DelegatePublicKey::default();
        let entity = DelegatePreConfirmationKey {
            public_key: delegate_key,
            expiration: Tai64(1234u64),
        };
        let sealed = Sealed { signature, entity };
        let inner = P2PPreConfirmationMessage::Delegate(sealed);
        GossipData {
            data: Some(inner),
            peer_id: Default::default(),
            message_id: vec![],
        }
    }

    fn arbitrary_pre_confirmation_message(
        tx_ids: &[TxId],
    ) -> P2PPreConfirmationGossipData {
        let preconfirmations = tx_ids
            .iter()
            .map(|tx_id| Preconfirmation {
                tx_id: *tx_id,
                status: PreconfirmationStatus::Success {
                    tx_pointer: Default::default(),
                    total_gas: 0,
                    total_fee: 0,
                    receipts: vec![],
                    outputs: vec![],
                },
            })
            .collect::<Vec<_>>();
        let entity = Preconfirmations {
            preconfirmations,
            expiration: Tai64(1234u64),
        };
        let signature = Bytes64::from([1u8; 64]);
        let sealed = Sealed { signature, entity };
        let inner = P2PPreConfirmationMessage::Preconfirmations(sealed);
        GossipData {
            data: Some(inner),
            peer_id: Default::default(),
            message_id: vec![],
        }
    }

    #[tokio::test]
    async fn run__when_receive_pre_confirmation_delegations_message_updates_delegate() {
        // given
        let (signature_verification, mut new_delegate_handle) =
            FakeSignatureVerification::new_with_handles(true);
        let (mut task, handles) = new_task_with_handles(signature_verification);
        let delegate_signature_message = arbitrary_delegate_signatures_message();
        let mut state_watcher = StateWatcher::started();

        // when
        tokio::task::spawn(async move {
            let _ = task.run(&mut state_watcher).await;
        });
        handles
            .pre_confirmation_updates
            .send(delegate_signature_message.clone())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        let actual = new_delegate_handle
            .new_delegate_receiver
            .recv()
            .await
            .unwrap();
        let expected = if let PreConfirmationMessage::Delegate(delegate_info) =
            delegate_signature_message.data.unwrap()
        {
            delegate_info
        } else {
            panic!("Expected Delegate message");
        };
        assert_eq!(actual, expected);
    }

    async fn all_streams_return_success(streams: Vec<TxStatusStream>) -> bool {
        for mut stream in streams {
            let msg = stream.next().await.unwrap();
            match msg {
                TxStatusMessage::Status(_) => {
                    // should be good if we get this
                }
                _ => return false,
            }
        }
        true
    }

    #[tokio::test]
    async fn run__when_pre_confirmations_pass_verification_then_send() {
        // given
        let (signature_verification, _) =
            FakeSignatureVerification::new_with_handles(true);
        let (mut task, handles) = new_task_with_handles(signature_verification);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let pre_confirmation_message = arbitrary_pre_confirmation_message(&tx_ids);
        let mut state_watcher = StateWatcher::started();

        let streams = tx_ids
            .iter()
            .map(|tx_id| {
                handles
                    .update_sender
                    .try_subscribe::<MpscChannel>(*tx_id)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        // when
        tokio::task::spawn(async move {
            let _ = task.run(&mut state_watcher).await;
        });
        handles
            .pre_confirmation_updates
            .send(pre_confirmation_message.clone())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        all_streams_return_success(streams).await;
    }

    #[tokio::test]
    async fn run__when_pre_confirmations_fail_verification_then_do_not_send() {
        // given
        let (signature_verification, _) =
            FakeSignatureVerification::new_with_handles(false);
        let (mut task, handles) = new_task_with_handles(signature_verification);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let pre_confirmation_message = arbitrary_pre_confirmation_message(&tx_ids);
        let mut state_watcher = StateWatcher::started();

        let streams = tx_ids
            .iter()
            .map(|tx_id| {
                handles
                    .update_sender
                    .try_subscribe::<MpscChannel>(*tx_id)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        // when
        tokio::task::spawn(async move {
            let _ = task.run(&mut state_watcher).await;
        });
        handles
            .pre_confirmation_updates
            .send(pre_confirmation_message.clone())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        for mut stream in streams {
            let res =
                tokio::time::timeout(Duration::from_millis(100), stream.next()).await;
            assert!(res.is_err());
        }
    }

    #[tokio::test]
    async fn run__when_pre_confirmations_fail_verification_they_can_be_retried_on_next_delegate_update(
    ) {
        // given
        let (signature_verification, mut fake_signature_handles) =
            FakeSignatureVerification::new_with_handles(false);
        let (task, handles) = new_task_with_handles(signature_verification);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let pre_confirmation_message = arbitrary_pre_confirmation_message(&tx_ids);

        let mut streams = tx_ids
            .iter()
            .map(|tx_id| {
                handles
                    .update_sender
                    .try_subscribe::<MpscChannel>(*tx_id)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();
        handles
            .pre_confirmation_updates
            .send(pre_confirmation_message.clone())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        for stream in &mut streams {
            let res =
                tokio::time::timeout(Duration::from_millis(100), stream.next()).await;
            assert!(res.is_err());
        }

        // when
        let new_delegate_message = arbitrary_delegate_signatures_message();
        fake_signature_handles.update_signature_verification_result(true);
        handles
            .pre_confirmation_updates
            .send(new_delegate_message)
            .await
            .unwrap();

        // then
        all_streams_return_success(streams).await;
    }
}
