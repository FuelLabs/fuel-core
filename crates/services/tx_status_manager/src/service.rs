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
    ed25519::Signature,
    ed25519_dalek::Verifier,
    fuel_crypto::Message,
    fuel_tx::{
        Address,
        Bytes32,
        Bytes64,
        Input,
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
    tai64::Tai64,
};
use futures::StreamExt;
use std::collections::HashMap;
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

pub struct Task<Pubkey> {
    manager: TxStatusManager,
    subscriptions: Subscriptions,
    read_requests_receiver: mpsc::Receiver<ReadRequest>,
    write_requests_receiver: mpsc::UnboundedReceiver<WriteRequest>,
    shared_data: SharedData,
    signature_verification: SignatureVerification<Pubkey>,
}

pub trait ProtocolPublicKey: Send {
    fn latest_address(&self) -> Address;
}

struct SignatureVerification<Pubkey> {
    protocol_pubkey: Pubkey,
    delegate_keys: HashMap<Tai64, DelegatePublicKey>,
}

impl<Pubkey: ProtocolPublicKey> SignatureVerification<Pubkey> {
    pub fn new(protocol_pubkey: Pubkey) -> Self {
        Self {
            protocol_pubkey,
            delegate_keys: HashMap::new(),
        }
    }

    fn verify_preconfirmation(
        delegate_key: &DelegatePublicKey,
        sealed: &Sealed<Preconfirmations, Bytes64>,
    ) -> bool {
        let bytes = match postcard::to_allocvec(&sealed.entity) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!("Failed to serialize preconfirmation: {e:?}");
                return false;
            }
        };

        let signature = Signature::from_bytes(&sealed.signature);
        match delegate_key.verify(&bytes, &signature) {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("Failed to verify preconfirmation signature: {e:?}");
                false
            }
        }
    }

    fn remove_expired_delegates(&mut self) {
        let now = Tai64::now();
        self.delegate_keys.retain(|exp, _| exp > &now);
    }

    fn add_new_delegate(
        &mut self,
        sealed: &Sealed<DelegatePreConfirmationKey<DelegatePublicKey>, ProtocolSignature>,
    ) -> bool {
        let Sealed { entity, signature } = sealed;
        let bytes = postcard::to_allocvec(&entity).unwrap();
        let message = Message::new(&bytes);
        let expected_address = self.protocol_pubkey.latest_address();
        let verified = signature
            .recover(&message)
            .map_or(false, |pubkey| Input::owner(&pubkey) == expected_address);
        self.remove_expired_delegates();
        if verified {
            self.delegate_keys
                .insert(entity.expiration, entity.public_key);
        };
        verified
    }

    fn check_preconfirmation_signature(
        &mut self,
        sealed: &Sealed<Preconfirmations, Bytes64>,
    ) -> bool {
        let expiration = sealed.entity.expiration;
        let now = Tai64::now();
        if now > expiration {
            tracing::warn!("Preconfirmation signature expired: {now:?} > {expiration:?}");
            return false;
        }
        self.delegate_keys
            .get(&expiration)
            .map(|delegate_key| Self::verify_preconfirmation(delegate_key, sealed))
            .unwrap_or(false)
    }
}

impl<Pubkey: ProtocolPublicKey> Task<Pubkey> {
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

    fn new_preconfirmations_from_p2p(
        &mut self,
        preconfirmations: P2PPreConfirmationMessage,
    ) {
        match preconfirmations {
            PreConfirmationMessage::Delegate(sealed) => {
                tracing::debug!(
                    "Received new delegate signature from peer: {:?}",
                    sealed.entity.public_key
                );
                // TODO: Report peer for sending invalid delegation
                //  https://github.com/FuelLabs/fuel-core/issues/2872
                let _ = self.signature_verification.add_new_delegate(&sealed);
            }
            PreConfirmationMessage::Preconfirmations(sealed) => {
                tracing::debug!("Received new preconfirmations from peer");
                if self
                    .signature_verification
                    .check_preconfirmation_signature(&sealed)
                {
                    self.handle_verified_preconfirmation(sealed);
                } else {
                    // There is a chance that this is a signature for whom the delegate key hasn't
                    // arrived yet, in which case the pre-confirmation will be lost
                    tracing::warn!("Preconfirmation signature verification failed");

                    // TODO: Report peer for sending invalid preconfirmation
                    //  https://github.com/FuelLabs/fuel-core/issues/2872
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl<Pubkey: ProtocolPublicKey> RunnableService for Task<Pubkey> {
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

impl<Pubkey: ProtocolPublicKey> RunnableTask for Task<Pubkey> {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }

            tx_status_from_p2p = self.subscriptions.new_tx_status.next() => {
                if let Some(GossipData { data, .. }) = tx_status_from_p2p {
                    if let Some(msg) = data {
                        self.new_preconfirmations_from_p2p(msg);
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

pub fn new_service<P2P, Pubkey>(
    p2p: P2P,
    config: Config,
    protocol_pubkey: Pubkey,
) -> ServiceRunner<Task<Pubkey>>
where
    P2P: P2PSubscriptions<GossipedStatuses = P2PPreConfirmationGossipData>,
    Pubkey: ProtocolPublicKey,
{
    let tx_status_from_p2p_stream = p2p.gossiped_tx_statuses();
    let subscriptions = Subscriptions {
        new_tx_status: tx_status_from_p2p_stream,
    };

    let tx_status_sender =
        TxStatusChange::new(config.max_tx_update_subscriptions, config.subscription_ttl);
    let tx_status_manager =
        TxStatusManager::new(tx_status_sender, config.status_cache_ttl, config.metrics);

    let (read_requests_sender, read_requests_receiver) =
        mpsc::channel(config.max_tx_update_subscriptions);
    let (write_requests_sender, write_requests_receiver) = mpsc::unbounded_channel();

    let shared_data = SharedData {
        read_requests_sender,
        write_requests_sender,
    };
    let signature_verification = SignatureVerification::new(protocol_pubkey);

    ServiceRunner::new(Task {
        subscriptions,
        manager: tx_status_manager,
        read_requests_receiver,
        write_requests_receiver,
        shared_data,
        signature_verification,
    })
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use super::*;
    use crate::{
        update_sender::{
            MpscChannel,
            UpdateSender,
        },
        TxStatusMessage,
    };
    use fuel_core_services::Service;
    use fuel_core_types::{
        ed25519_dalek::{
            Signer,
            SigningKey as DalekSigningKey,
            VerifyingKey as DalekVerifyingKey,
        },
        fuel_crypto::{
            Message,
            PublicKey,
            SecretKey,
            Signature,
        },
        services::{
            p2p::{
                DelegatePreConfirmationKey,
                Tai64,
            },
            preconfirmation::{
                PreconfirmationStatus,
                Preconfirmations,
            },
        },
    };
    use std::time::Duration;
    use tokio_stream::wrappers::ReceiverStream;

    const TTL: Duration = Duration::from_secs(4);

    struct Handles {
        pub pre_confirmation_updates: mpsc::Sender<P2PPreConfirmationGossipData>,
        pub update_sender: UpdateSender,
        pub protocol_signing_key: SecretKey,
    }

    fn new_task_with_handles() -> (Task<PublicKey>, Handles) {
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
        let tx_status_manager = TxStatusManager::new(tx_status_change, TTL, false);
        let signing_key = SecretKey::default();
        let protocol_public_key = signing_key.public_key();
        let signature_verification = SignatureVerification::new(protocol_public_key);
        let task = Task {
            manager: tx_status_manager,
            subscriptions,
            read_requests_receiver,
            write_requests_receiver,
            shared_data,
            signature_verification,
        };
        let handles = Handles {
            pre_confirmation_updates: sender,
            update_sender: updater_sender,
            protocol_signing_key: signing_key,
        };
        (task, handles)
    }

    async fn all_streams_return_success(streams: Vec<TxStatusStream>) -> bool {
        for mut stream in streams {
            let timeout = Duration::from_millis(100);
            let msg = tokio::time::timeout(timeout, stream.next())
                .await
                .unwrap_or_else(|_| panic!("This should not timeout: {timeout:?}"))
                .unwrap();
            match msg {
                TxStatusMessage::Status(_) => {
                    // should be good if we get this
                }
                _ => return false,
            }
        }
        true
    }

    async fn all_streams_timeout(streams: &mut Vec<TxStatusStream>) -> bool {
        for stream in streams {
            let timeout = Duration::from_millis(100);
            let res = tokio::time::timeout(timeout, stream.next()).await;
            if res.is_ok() {
                return false;
            }
        }
        true
    }

    fn valid_sealed_delegate_signature(
        protocol_secret_key: SecretKey,
        delegate_public_key: DelegatePublicKey,
        expiration: Tai64,
    ) -> P2PPreConfirmationGossipData {
        let entity = DelegatePreConfirmationKey {
            public_key: delegate_public_key,
            expiration,
        };
        let bytes = postcard::to_allocvec(&entity).unwrap();
        let message = Message::new(&bytes);
        let signature = Signature::sign(&protocol_secret_key, &message);
        let sealed = Sealed { entity, signature };
        let inner = P2PPreConfirmationMessage::Delegate(sealed);
        GossipData {
            data: Some(inner),
            peer_id: Default::default(),
            message_id: vec![],
        }
    }

    fn valid_pre_confirmation_signature(
        preconfirmations: Vec<Preconfirmation>,
        delegate_private_key: DalekSigningKey,
        expiration: Tai64,
    ) -> P2PPreConfirmationGossipData {
        let entity = Preconfirmations {
            expiration,
            preconfirmations,
        };
        let bytes = postcard::to_allocvec(&entity).unwrap();
        let typed_signature = delegate_private_key.sign(&bytes);
        let signature = Bytes64::new(typed_signature.to_bytes());
        let sealed = Sealed { entity, signature };
        let inner = P2PPreConfirmationMessage::Preconfirmations(sealed);
        GossipData {
            data: Some(inner),
            peer_id: Default::default(),
            message_id: vec![],
        }
    }

    fn bad_pre_confirmation_signature(
        preconfirmations: Vec<Preconfirmation>,
        delegate_private_key: DalekSigningKey,
        expiration: Tai64,
    ) -> P2PPreConfirmationGossipData {
        let mut mutated_private_key = delegate_private_key.to_bytes();
        for byte in mutated_private_key.iter_mut() {
            *byte = byte.wrapping_add(1);
        }
        let mutated_delegate_private_key =
            DalekSigningKey::from_bytes(&mutated_private_key);
        valid_pre_confirmation_signature(
            preconfirmations,
            mutated_delegate_private_key,
            expiration,
        )
    }

    fn delegate_key_pair() -> (DalekSigningKey, DalekVerifyingKey) {
        let secret_key = [99u8; 32];
        let secret_key = DalekSigningKey::from_bytes(&secret_key);
        let public_key = secret_key.verifying_key();
        (secret_key, public_key)
    }

    #[tokio::test]
    async fn run__when_pre_confirmations_pass_verification_then_send() {
        // given
        let (task, handles) = new_task_with_handles();

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: PreconfirmationStatus::Success {
                    tx_pointer: Default::default(),
                    total_gas: 0,
                    total_fee: 0,
                    receipts: vec![],
                    outputs: vec![],
                },
            })
            .collect();
        let (delegate_signing_key, delegate_verifying_key) = delegate_key_pair();
        let expiration = Tai64(u64::MAX);
        let pre_confirmation_message = valid_pre_confirmation_signature(
            preconfirmations,
            delegate_signing_key,
            expiration,
        );
        let delegate_signature_message = valid_sealed_delegate_signature(
            handles.protocol_signing_key,
            delegate_verifying_key,
            expiration,
        );

        let streams = tx_ids
            .iter()
            .map(|tx_id| {
                handles
                    .update_sender
                    .try_subscribe::<MpscChannel>(*tx_id)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        handles
            .pre_confirmation_updates
            .send(delegate_signature_message)
            .await
            .unwrap();

        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // when
        handles
            .pre_confirmation_updates
            .send(pre_confirmation_message.clone())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        assert!(all_streams_return_success(streams).await);
    }

    #[tokio::test]
    async fn run__when_pre_confirmations_unknown_delegate_key_then_do_not_send() {
        // given
        let (task, handles) = new_task_with_handles();

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: PreconfirmationStatus::Success {
                    tx_pointer: Default::default(),
                    total_gas: 0,
                    total_fee: 0,
                    receipts: vec![],
                    outputs: vec![],
                },
            })
            .collect();
        let (delegate_signing_key, _) = delegate_key_pair();
        let expiration = Tai64(u64::MAX);
        let invalid_pre_confirmation_message = bad_pre_confirmation_signature(
            preconfirmations,
            delegate_signing_key,
            expiration,
        );

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

        // when
        handles
            .pre_confirmation_updates
            .send(invalid_pre_confirmation_message)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        assert!(all_streams_timeout(&mut streams).await);
    }

    #[tokio::test]
    async fn run__when_pre_confirmations_bad_signature_then_do_not_send() {
        // given
        let (task, handles) = new_task_with_handles();

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: PreconfirmationStatus::Success {
                    tx_pointer: Default::default(),
                    total_gas: 0,
                    total_fee: 0,
                    receipts: vec![],
                    outputs: vec![],
                },
            })
            .collect();
        let (delegate_signing_key, delegate_verifying_key) = delegate_key_pair();
        let expiration = Tai64(u64::MAX);
        let invalid_pre_confirmation_message = bad_pre_confirmation_signature(
            preconfirmations,
            delegate_signing_key,
            expiration,
        );
        let delegate_signature_message = valid_sealed_delegate_signature(
            handles.protocol_signing_key,
            delegate_verifying_key,
            expiration,
        );

        let mut streams = tx_ids
            .iter()
            .map(|tx_id| {
                handles
                    .update_sender
                    .try_subscribe::<MpscChannel>(*tx_id)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        handles
            .pre_confirmation_updates
            .send(delegate_signature_message)
            .await
            .unwrap();

        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // when
        handles
            .pre_confirmation_updates
            .send(invalid_pre_confirmation_message)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        assert!(all_streams_timeout(&mut streams).await);
    }

    #[tokio::test]
    async fn run__if_preconfirmation_signature_is_expired_do_not_send() {
        // given
        let (task, handles) = new_task_with_handles();

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: PreconfirmationStatus::Success {
                    tx_pointer: Default::default(),
                    total_gas: 0,
                    total_fee: 0,
                    receipts: vec![],
                    outputs: vec![],
                },
            })
            .collect();
        let (delegate_signing_key, delegate_verifying_key) = delegate_key_pair();
        let expiration = Tai64::now();
        let pre_confirmation_message = valid_pre_confirmation_signature(
            preconfirmations,
            delegate_signing_key,
            expiration,
        );

        let delegate_signature_message = valid_sealed_delegate_signature(
            handles.protocol_signing_key,
            delegate_verifying_key,
            expiration,
        );

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
            .send(delegate_signature_message)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_secs(1)).await;

        // when
        handles
            .pre_confirmation_updates
            .send(pre_confirmation_message)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        assert!(all_streams_timeout(&mut streams).await);
    }

    #[tokio::test]
    async fn run__can_verify_preconfirmation_signature_while_tracking_multiple_delegate_keys(
    ) {
        // given
        let (task, handles) = new_task_with_handles();
        let (delegate_secret_key, delegate_public_key) = delegate_key_pair();
        let first_expiration = Tai64(u64::MAX - 200);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: PreconfirmationStatus::Success {
                    tx_pointer: Default::default(),
                    total_gas: 0,
                    total_fee: 0,
                    receipts: vec![],
                    outputs: vec![],
                },
            })
            .collect();
        let valid_pre_confirmation_message = valid_pre_confirmation_signature(
            preconfirmations,
            delegate_secret_key,
            first_expiration,
        );

        let streams = tx_ids
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

        for expiration_modifier in 0..100u64 {
            let expiration = first_expiration + expiration_modifier;
            let valid_delegate_signature = valid_sealed_delegate_signature(
                handles.protocol_signing_key,
                delegate_public_key,
                expiration,
            );
            handles
                .pre_confirmation_updates
                .send(valid_delegate_signature)
                .await
                .unwrap();
        }

        // when
        handles
            .pre_confirmation_updates
            .send(valid_pre_confirmation_message)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        assert!(all_streams_return_success(streams).await);
    }
}
