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
        transaction_status::{
            statuses,
            PreConfirmationStatus,
            TransactionStatus,
        },
    },
    tai64::Tai64,
};
use futures::StreamExt;
use std::collections::HashMap;
use tokio::sync::{
    broadcast,
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

enum UpdateRequest {
    Status {
        tx_id: TxId,
        status: TransactionStatus,
    },
    Statuses {
        statuses: Vec<(TxId, statuses::SqueezedOut)>,
    },
    Preconfirmations {
        preconfirmations: Vec<Preconfirmation>,
    },
}

pub struct SharedData {
    read_requests_sender: mpsc::Sender<ReadRequest>,
    write_requests_sender: mpsc::UnboundedSender<UpdateRequest>,
    tx_status_receiver: broadcast::Receiver<(TxId, PreConfirmationStatus)>,
}

impl Clone for SharedData {
    fn clone(&self) -> Self {
        Self {
            read_requests_sender: self.read_requests_sender.clone(),
            write_requests_sender: self.write_requests_sender.clone(),
            tx_status_receiver: self.tx_status_receiver.resubscribe(),
        }
    }
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
        let request = UpdateRequest::Status { tx_id, status };
        let _ = self.write_requests_sender.send(request);
    }

    pub fn update_statuses(&self, statuses: Vec<(TxId, statuses::SqueezedOut)>) {
        let request = UpdateRequest::Statuses { statuses };
        let _ = self.write_requests_sender.send(request);
    }

    pub fn update_preconfirmations(&self, preconfirmations: Vec<Preconfirmation>) {
        let request = UpdateRequest::Preconfirmations { preconfirmations };
        if let Err(e) = self.write_requests_sender.send(request) {
            tracing::error!("Failed to send preconfirmations: {:?}", e);
        }
    }

    pub fn preconfirmations_update_listener(
        &self,
    ) -> broadcast::Receiver<(TxId, PreConfirmationStatus)> {
        self.tx_status_receiver.resubscribe()
    }
}

pub struct Task<Pubkey> {
    manager: TxStatusManager,
    subscriptions: Subscriptions,
    read_requests_receiver: mpsc::Receiver<ReadRequest>,
    write_requests_receiver: mpsc::UnboundedReceiver<UpdateRequest>,
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
    fn handle_preconfirmations(&mut self, preconfirmations: Vec<Preconfirmation>) {
        preconfirmations
            .into_iter()
            .for_each(|Preconfirmation { tx_id, status }| {
                let status: TransactionStatus = status.into();
                self.manager.status_update(tx_id, status);
            });
    }

    fn new_preconfirmations_from_p2p(
        &mut self,
        preconfirmations: P2PPreConfirmationMessage,
    ) {
        match preconfirmations {
            PreConfirmationMessage::Delegate { seal, .. } => {
                tracing::debug!(
                    "Received new delegate signature from peer: {:?}",
                    seal.entity.public_key
                );
                // TODO: Report peer for sending invalid delegation
                //  https://github.com/FuelLabs/fuel-core/issues/2872
                let _ = self.signature_verification.add_new_delegate(&seal);
            }
            PreConfirmationMessage::Preconfirmations(sealed) => {
                tracing::debug!("Received new preconfirmations from peer");
                if self
                    .signature_verification
                    .check_preconfirmation_signature(&sealed)
                {
                    tracing::debug!("Preconfirmation signature verified");
                    let Sealed { entity, .. } = sealed;
                    self.handle_preconfirmations(entity.preconfirmations);
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
                    Some(UpdateRequest::Status { tx_id, status }) => {
                        self.manager.status_update(tx_id, status);
                        TaskNextAction::Continue
                    }
                    Some(UpdateRequest::Statuses { statuses }) => {
                        for (tx_id, status) in statuses {
                            self.manager.status_update(tx_id, status.into());
                        }
                        TaskNextAction::Continue
                    }
                    Some(UpdateRequest::Preconfirmations { preconfirmations }) => {
                        self.handle_preconfirmations(preconfirmations);
                        TaskNextAction::Continue
                    }
                    None => {
                        TaskNextAction::Stop
                    },
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
                        // TODO[RC]: This part not tested by TxStatusManager service tests yet.
                        let result = self.manager.tx_update_subscribe(tx_id);
                        let _ = sender.send(result);
                        TaskNextAction::Continue
                    }
                    None => {
                        TaskNextAction::Stop
                    },
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
    let (tx_status_sender, tx_status_receiver) =
        broadcast::channel(config.max_tx_update_subscriptions);
    let subscriptions = Subscriptions {
        new_tx_status: tx_status_from_p2p_stream,
    };

    let tx_update_sender =
        TxStatusChange::new(config.max_tx_update_subscriptions, config.subscription_ttl);
    let tx_status_manager = TxStatusManager::new(
        tx_status_sender,
        tx_update_sender,
        config.status_cache_ttl,
        config.metrics,
    );

    let (read_requests_sender, read_requests_receiver) =
        mpsc::channel(config.max_tx_update_subscriptions);
    let (write_requests_sender, write_requests_receiver) = mpsc::unbounded_channel();

    let shared_data = SharedData {
        read_requests_sender,
        write_requests_sender,
        tx_status_receiver,
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
#[allow(non_snake_case)]
mod tests {
    use std::{
        collections::HashSet,
        time::Duration,
    };

    use fuel_core_services::{
        Service,
        ServiceRunner,
    };
    use fuel_core_types::{
        fuel_crypto::{
            rand::{
                rngs::StdRng,
                SeedableRng,
            },
            Message,
            PublicKey,
            SecretKey,
            Signature,
        },
        fuel_tx::{
            Bytes32,
            Bytes64,
        },
        services::{
            p2p::{
                DelegatePreConfirmationKey,
                DelegatePublicKey,
                GossipData,
                Sealed,
            },
            preconfirmation::{
                Preconfirmation,
                Preconfirmations,
            },
            transaction_status::TransactionStatus,
        },
        tai64::Tai64,
    };
    use futures::{
        stream::BoxStream,
        StreamExt,
    };
    use status::transaction::{
        random_prunable_tx_status,
        random_tx_status,
    };
    use tokio::{
        sync::{
            broadcast,
            mpsc,
            oneshot,
        },
        time::Instant,
    };
    use tokio_stream::wrappers::ReceiverStream;

    use crate::{
        manager::TxStatusManager,
        ports::{
            P2PPreConfirmationGossipData,
            P2PPreConfirmationMessage,
        },
        subscriptions::Subscriptions,
        update_sender::{
            MpscChannel,
            TxStatusChange,
            UpdateSender,
        },
        TxStatusMessage,
        TxStatusStream,
    };

    use super::{
        ReadRequest,
        SharedData,
        SignatureVerification,
        Task,
        UpdateRequest,
    };
    use fuel_core_types::ed25519_dalek::{
        Signer,
        SigningKey as DalekSigningKey,
        VerifyingKey as DalekVerifyingKey,
    };

    const MORE_THAN_TTL: Duration = Duration::from_secs(5);
    const TTL: Duration = Duration::from_secs(4);
    const HALF_OF_TTL: Duration = Duration::from_secs(2);
    const QUART_OF_TTL: Duration = Duration::from_secs(1);

    struct Handles {
        pub pre_confirmation_updates: mpsc::Sender<GossipData<P2PPreConfirmationMessage>>,
        pub write_requests_sender: mpsc::UnboundedSender<UpdateRequest>,
        pub read_requests_sender: mpsc::Sender<ReadRequest>,
        pub tx_status_change: TxStatusChange,
        pub update_sender: UpdateSender,
        pub protocol_signing_key: SecretKey,
    }

    pub(super) mod status {
        pub(super) mod preconfirmation {

            use fuel_core_types::services::preconfirmation::PreconfirmationStatus;

            pub fn success() -> PreconfirmationStatus {
                PreconfirmationStatus::Success {
                    tx_pointer: Default::default(),
                    total_gas: Default::default(),
                    total_fee: Default::default(),
                    receipts: vec![],
                    outputs: vec![],
                }
            }
        }

        pub(super) mod transaction {
            use std::sync::Arc;

            use fuel_core_types::{
                fuel_crypto::rand::{
                    rngs::StdRng,
                    seq::SliceRandom,
                },
                services::transaction_status::{
                    statuses::{
                        PreConfirmationFailure,
                        PreConfirmationSuccess,
                    },
                    TransactionStatus,
                },
                tai64::Tai64,
            };

            use crate::manager::TxStatusManager;

            pub fn submitted() -> TransactionStatus {
                TransactionStatus::submitted(Tai64::UNIX_EPOCH)
            }

            pub fn success() -> TransactionStatus {
                TransactionStatus::Success(Default::default())
            }

            pub fn preconfirmation_success() -> TransactionStatus {
                let inner = PreConfirmationSuccess {
                    tx_pointer: Default::default(),
                    total_gas: Default::default(),
                    total_fee: Default::default(),
                    receipts: Some(vec![]),
                    resolved_outputs: Some(vec![]),
                };
                TransactionStatus::PreConfirmationSuccess(Arc::new(inner))
            }

            pub fn squeezed_out() -> TransactionStatus {
                TransactionStatus::squeezed_out("fishy tx".to_string())
            }

            pub fn preconfirmation_squeezed_out() -> TransactionStatus {
                TransactionStatus::preconfirmation_squeezed_out(
                    "fishy preconfirmation".to_string(),
                )
            }

            pub fn failure() -> TransactionStatus {
                TransactionStatus::Failure(Default::default())
            }

            pub fn preconfirmation_failure() -> TransactionStatus {
                let inner = PreConfirmationFailure {
                    tx_pointer: Default::default(),
                    total_gas: Default::default(),
                    total_fee: Default::default(),
                    receipts: Some(vec![]),
                    resolved_outputs: Some(vec![]),
                    reason: "None".to_string(),
                };
                TransactionStatus::PreConfirmationFailure(Arc::new(inner))
            }

            pub fn all_statuses() -> [TransactionStatus; 7] {
                [
                    submitted(),
                    success(),
                    preconfirmation_success(),
                    squeezed_out(),
                    preconfirmation_squeezed_out(),
                    failure(),
                    preconfirmation_failure(),
                ]
            }

            pub fn random_prunable_tx_status(rng: &mut StdRng) -> TransactionStatus {
                all_statuses()
                    .into_iter()
                    .filter(TxStatusManager::is_prunable)
                    .collect::<Vec<_>>()
                    .choose(rng)
                    .unwrap()
                    .clone()
            }

            pub fn random_tx_status(rng: &mut StdRng) -> TransactionStatus {
                all_statuses().choose(rng).unwrap().clone()
            }
        }
    }

    fn new_task_with_handles(ttl: Duration) -> (Task<PublicKey>, Handles) {
        let (read_requests_sender, read_requests_receiver) = mpsc::channel(1);
        let (write_requests_sender, write_requests_receiver) = mpsc::unbounded_channel();
        let (tx_status_sender, tx_status_receiver) = broadcast::channel(10000);
        let shared_data = SharedData {
            read_requests_sender: read_requests_sender.clone(),
            write_requests_sender: write_requests_sender.clone(),
            tx_status_receiver,
        };

        let (sender, receiver) = mpsc::channel(1_000);
        let new_tx_status = Box::pin(ReceiverStream::new(receiver));
        let subscriptions = Subscriptions { new_tx_status };
        let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
        let signing_key = SecretKey::default();
        let protocol_public_key = signing_key.public_key();
        let signature_verification = SignatureVerification::new(protocol_public_key);
        let update_sender = tx_status_change.update_sender.clone();
        let manager =
            TxStatusManager::new(tx_status_sender, tx_status_change.clone(), ttl, false);

        let handles = Handles {
            pre_confirmation_updates: sender,
            tx_status_change,
            write_requests_sender,
            read_requests_sender,
            update_sender,
            protocol_signing_key: signing_key,
        };

        let task = Task {
            subscriptions,
            manager,
            read_requests_receiver,
            write_requests_receiver,
            shared_data,
            signature_verification,
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
        let seal = Sealed { entity, signature };
        let inner = P2PPreConfirmationMessage::Delegate { seal, nonce: 0 };
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

    async fn send_status_updates(
        updates: &[(Bytes32, TransactionStatus)],
        sender: &mpsc::UnboundedSender<UpdateRequest>,
    ) {
        updates.iter().for_each(|(tx_id, status)| {
            sender
                .send(UpdateRequest::Status {
                    tx_id: *tx_id,
                    status: status.clone(),
                })
                .unwrap();
        });
        tokio::time::advance(Duration::from_millis(100)).await;
    }

    fn pruning_tx_id() -> Bytes32 {
        let marker: u64 = 0xDEADBEEF;
        let mut id = [0u8; 32];
        id[0..8].copy_from_slice(&marker.to_le_bytes());
        id.into()
    }

    async fn force_pruning(sender: &mpsc::UnboundedSender<UpdateRequest>) {
        let id = pruning_tx_id();
        sender
            .send(UpdateRequest::Status {
                tx_id: id,
                status: status::transaction::failure(),
            })
            .unwrap();
        tokio::time::advance(Duration::from_millis(100)).await;
    }

    async fn assert_presence_with_status(
        status_read: &mpsc::Sender<ReadRequest>,
        txs: Vec<(Bytes32, TransactionStatus)>,
    ) {
        for (id, status) in txs.iter() {
            let response = get_status(status_read, id).await;
            assert_eq!(response, Some(status.clone()));
        }
    }

    async fn get_status(
        status_read: &mpsc::Sender<ReadRequest>,
        id: &Bytes32,
    ) -> Option<TransactionStatus> {
        let (sender, receiver) = oneshot::channel();
        status_read
            .send(ReadRequest::GetStatus {
                tx_id: (*id),
                sender,
            })
            .await
            .unwrap();

        receiver.await.unwrap()
    }

    async fn assert_status<F>(
        status_read: &mpsc::Sender<ReadRequest>,
        txs: Vec<Bytes32>,
        pred: F,
    ) where
        F: Fn(Option<TransactionStatus>) -> bool,
    {
        for id in txs.iter() {
            let (sender, receiver) = oneshot::channel();
            status_read
                .send(ReadRequest::GetStatus {
                    tx_id: (*id),
                    sender,
                })
                .await
                .unwrap();

            let response = receiver.await.unwrap();
            assert!(pred(response));
        }
    }

    async fn assert_presence(status_read: &mpsc::Sender<ReadRequest>, txs: Vec<Bytes32>) {
        assert_status(status_read, txs, |s| s.is_some()).await;
    }

    async fn assert_absence(status_read: &mpsc::Sender<ReadRequest>, txs: Vec<Bytes32>) {
        assert_status(status_read, txs, |s| s.is_none()).await;
    }

    async fn assert_status_change_notifications(
        validators: &[for<'a> fn(&'a TransactionStatus) -> bool],
        mut stream: BoxStream<'_, TxStatusMessage>,
    ) {
        let mut received_statuses = vec![];
        let timeout_duration = Duration::from_millis(250);
        while let Ok(Some(message)) =
            tokio::time::timeout(timeout_duration, stream.next()).await
        {
            match message {
                TxStatusMessage::Status(s) => received_statuses.push(s),
                TxStatusMessage::FailedStatus => {
                    panic!("should not happen");
                }
            }
        }

        assert_eq!(received_statuses.len(), validators.len(), "Length mismatch");
        for (item, &validator) in received_statuses.iter().zip(validators.iter()) {
            assert!(validator(item));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn run__can_store_and_retrieve_all_statuses() {
        let (task, handles) = new_task_with_handles(TTL);
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id = [1u8; 32].into();
        let tx2_id = [2u8; 32].into();
        let tx3_id = [3u8; 32].into();
        let tx4_id = [4u8; 32].into();
        let tx5_id = [5u8; 32].into();
        let tx6_id = [6u8; 32].into();
        let tx7_id = [7u8; 32].into();
        let status_updates = vec![
            (tx1_id, status::transaction::submitted()),
            (tx2_id, status::transaction::success()),
            (tx3_id, status::transaction::preconfirmation_success()),
            (tx4_id, status::transaction::squeezed_out()),
            (tx5_id, status::transaction::preconfirmation_squeezed_out()),
            (tx6_id, status::transaction::failure()),
            (tx7_id, status::transaction::preconfirmation_failure()),
        ];

        // When
        send_status_updates(&status_updates, &handles.write_requests_sender).await;

        // Then
        assert_presence_with_status(&handles.read_requests_sender, status_updates).await;

        service.stop_and_await().await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn run__non_prunable_is_returned_when_both_prunable_and_non_prunable_are_present(
    ) {
        let (task, handles) = new_task_with_handles(TTL);
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id = [1u8; 32].into();
        let status_updates = vec![
            (tx1_id, status::transaction::success()),
            (tx1_id, status::transaction::submitted()),
        ];

        // When
        send_status_updates(&status_updates, &handles.write_requests_sender).await;

        // Then
        assert_presence_with_status(
            &handles.read_requests_sender,
            vec![(tx1_id, status::transaction::submitted())],
        )
        .await;

        service.stop_and_await().await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn run__only_prunable_statuses_are_pruned() {
        let (task, handles) = new_task_with_handles(TTL);
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id = [1u8; 32].into();
        let tx2_id = [2u8; 32].into();
        let tx3_id = [3u8; 32].into();
        let tx4_id = [4u8; 32].into();
        let tx5_id = [5u8; 32].into();
        let tx6_id = [6u8; 32].into();
        let tx7_id = [7u8; 32].into();
        let status_updates = vec![
            (tx1_id, status::transaction::submitted()),
            (tx2_id, status::transaction::success()),
            (tx3_id, status::transaction::preconfirmation_success()),
            (tx4_id, status::transaction::squeezed_out()),
            (tx5_id, status::transaction::preconfirmation_squeezed_out()),
            (tx6_id, status::transaction::failure()),
            (tx7_id, status::transaction::preconfirmation_failure()),
        ];

        // When
        send_status_updates(&status_updates, &handles.write_requests_sender).await;
        tokio::time::advance(MORE_THAN_TTL).await;
        force_pruning(&handles.write_requests_sender).await;

        // Then
        assert_presence(&handles.read_requests_sender, vec![tx1_id]).await;
        assert_absence(
            &handles.read_requests_sender,
            vec![tx2_id, tx3_id, tx4_id, tx5_id, tx6_id, tx7_id],
        )
        .await;

        service.stop_and_await().await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn run__pruning_works_with_ttl_0() {
        let (task, handles) = new_task_with_handles(Duration::from_secs(0));
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id = [1u8; 32].into();
        let status_updates = vec![(tx1_id, status::transaction::success())];

        // When
        send_status_updates(&status_updates, &handles.write_requests_sender).await;
        force_pruning(&handles.write_requests_sender).await;

        // Then
        assert_absence(&handles.read_requests_sender, vec![tx1_id]).await;
    }

    #[tokio::test(start_paused = true)]
    async fn run__does_not_prune_when_ttl_not_passed() {
        let (task, handles) = new_task_with_handles(TTL);
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id = [1u8; 32].into();
        let status_updates = vec![(tx1_id, status::transaction::success())];

        // When
        send_status_updates(&status_updates, &handles.write_requests_sender).await;
        tokio::time::advance(HALF_OF_TTL).await;
        force_pruning(&handles.write_requests_sender).await;

        // Then
        assert_presence(&handles.read_requests_sender, vec![tx1_id]).await;
    }

    #[tokio::test(start_paused = true)]
    async fn run__prunes_when_the_same_tx_is_updated_from_non_prunable_to_prunable_status(
    ) {
        let (task, handles) = new_task_with_handles(TTL);
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id = [1u8; 32].into();
        let status_updates = vec![
            (tx1_id, status::transaction::submitted()),
            (tx1_id, status::transaction::success()),
        ];

        // When
        send_status_updates(&status_updates, &handles.write_requests_sender).await;
        tokio::time::advance(MORE_THAN_TTL).await;
        force_pruning(&handles.write_requests_sender).await;

        // Then
        assert_absence(&handles.read_requests_sender, vec![tx1_id]).await;
    }

    #[tokio::test(start_paused = true)]
    async fn run__status_update_resets_the_pruning_time() {
        let (task, handles) = new_task_with_handles(TTL);
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id = [1u8; 32].into();
        let tx2_id = [2u8; 32].into();
        let status_updates = vec![
            (tx1_id, status::transaction::success()),
            (tx2_id, status::transaction::success()),
        ];

        // When
        send_status_updates(&status_updates, &handles.write_requests_sender).await;
        tokio::time::advance(HALF_OF_TTL).await;

        let status_updates = vec![(tx1_id, status::transaction::failure())];
        send_status_updates(&status_updates, &handles.write_requests_sender).await;
        tokio::time::advance(HALF_OF_TTL + QUART_OF_TTL).await;
        force_pruning(&handles.write_requests_sender).await;

        // Then
        assert_presence_with_status(
            &handles.read_requests_sender,
            vec![(tx1_id, status::transaction::failure())],
        )
        .await;
        assert_absence(&handles.read_requests_sender, vec![tx2_id]).await;
    }

    #[tokio::test]
    async fn run__notifies_about_status_changes() {
        let (task, handles) = new_task_with_handles(TTL);
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id = [1u8; 32].into();
        let status_updates = [
            (tx1_id, status::transaction::submitted()),
            (tx1_id, status::transaction::success()),
        ];
        let stream = handles
            .tx_status_change
            .update_sender
            .try_subscribe::<MpscChannel>(tx1_id)
            .unwrap();

        // When
        status_updates.iter().for_each(|(tx_id, status)| {
            handles
                .write_requests_sender
                .send(UpdateRequest::Status {
                    tx_id: *tx_id,
                    status: status.clone(),
                })
                .unwrap();
        });

        // Then
        assert_status_change_notifications(
            &[
                |s| matches!(s, &TransactionStatus::Submitted(_)),
                |s| matches!(s, &TransactionStatus::Success(_)),
            ],
            stream,
        )
        .await;

        service.stop_and_await().await.unwrap();
    }

    use proptest::prelude::*;
    use std::collections::HashMap;

    const TX_ID_POOL_SIZE: usize = 20;
    const MIN_ACTIONS: usize = 50;
    const MAX_ACTIONS: usize = 1000;
    const MIN_TTL: u64 = 10;
    const MAX_TTL: u64 = 360;

    #[derive(Debug, Clone)]
    enum Action {
        UpdateStatus { tx_id_index: usize },
        AdvanceTime { seconds: u64 },
    }

    // How to select an ID from the pool
    fn tx_id_index_strategy(pool_size: usize) -> impl Strategy<Value = usize> {
        0..pool_size
    }

    // Possible values for TTL
    fn ttl_strategy(min_ttl: u64, max_ttl: u64) -> impl Strategy<Value = Duration> {
        (min_ttl..=max_ttl).prop_map(Duration::from_secs)
    }

    // Custom strategy to generate a sequence of actions
    fn actions_strategy(
        min_actions: usize,
        max_actions: usize,
    ) -> impl Strategy<Value = Vec<Action>> {
        let update_status_strategy = (tx_id_index_strategy(TX_ID_POOL_SIZE))
            .prop_map(|tx_id_index| Action::UpdateStatus { tx_id_index });

        let advance_time_strategy =
            (1..=MAX_TTL / 2).prop_map(|seconds| Action::AdvanceTime { seconds });

        prop::collection::vec(
            prop_oneof![update_status_strategy, advance_time_strategy],
            min_actions..max_actions,
        )
    }

    // Generate a pool of unique transaction IDs
    fn generate_tx_id_pool() -> Vec<[u8; 32]> {
        (0..TX_ID_POOL_SIZE)
            .map(|i| {
                let mut tx_id = [0u8; 32];
                tx_id[0..8].copy_from_slice(&(i as u64).to_le_bytes());
                tx_id
            })
            .collect()
    }

    #[tokio::main(start_paused = true, flavor = "current_thread")]
    #[allow(clippy::arithmetic_side_effects)]
    async fn _run__correctly_prunes_old_statuses(ttl: Duration, actions: Vec<Action>) {
        let mut rng = StdRng::seed_from_u64(2322u64);

        // Given
        let (task, handles) = new_task_with_handles(ttl);
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        let tx_id_pool = generate_tx_id_pool();

        // This will be used to track when each txid was updated so that
        // we can do the final assert against the TTL.
        let mut update_times = HashMap::new();
        let mut non_prunable_ids = HashSet::new();

        // When
        // Simulate flow of time and transaction updates
        for action in actions {
            match action {
                Action::UpdateStatus { tx_id_index } => {
                    let tx_id = tx_id_pool[tx_id_index];

                    // Make sure we'll never update back to submitted
                    let current_tx_status =
                        get_status(&handles.read_requests_sender, &tx_id.into()).await;
                    let new_tx_status = match current_tx_status {
                        Some(_) => random_prunable_tx_status(&mut rng),
                        None => random_tx_status(&mut rng),
                    };

                    if TxStatusManager::is_prunable(&new_tx_status) {
                        update_times.insert(tx_id, Instant::now());
                        non_prunable_ids.remove(&tx_id);
                    } else {
                        non_prunable_ids.insert(tx_id);
                    }
                    let status_updates = vec![(tx_id.into(), new_tx_status)];
                    send_status_updates(&status_updates, &handles.write_requests_sender)
                        .await;
                }
                Action::AdvanceTime { seconds } => {
                    tokio::time::advance(Duration::from_secs(seconds)).await;
                }
            }
        }

        // Trigger the final pruning, making sure we use ID that is not
        // in the pool
        force_pruning(&handles.write_requests_sender).await;
        update_times.insert(pruning_tx_id().into(), Instant::now());

        // Then
        // Verify that only expected statuses are present
        let (recent_tx_ids, not_recent_tx_ids): (Vec<_>, Vec<_>) = update_times
            .iter()
            .partition(|(_, &time)| time + ttl >= Instant::now());

        assert_presence(
            &handles.read_requests_sender,
            recent_tx_ids
                .into_iter()
                .map(|(tx_id, _)| (*tx_id).into())
                .chain(non_prunable_ids.iter().cloned().map(Into::into))
                .collect(),
        )
        .await;
        assert_absence(
            &handles.read_requests_sender,
            not_recent_tx_ids
                .into_iter()
                .filter(|(tx_id, _)| !non_prunable_ids.contains(*tx_id))
                .map(|(tx_id, _)| (*tx_id).into())
                .collect(),
        )
        .await;
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        #[allow(clippy::arithmetic_side_effects)]
        fn run__correctly_prunes_old_statuses(
            ttl in ttl_strategy(MIN_TTL, MAX_TTL),
            actions in actions_strategy(MIN_ACTIONS, MAX_ACTIONS)
        ) {
            _run__correctly_prunes_old_statuses(ttl, actions);
        }
    }

    #[tokio::test]
    async fn run__when_pre_confirmations_pass_verification_then_send() {
        // given
        let (task, handles) = new_task_with_handles(TTL);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: status::preconfirmation::success(),
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
        let (task, handles) = new_task_with_handles(TTL);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: status::preconfirmation::success(),
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
        let (task, handles) = new_task_with_handles(TTL);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: status::preconfirmation::success(),
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
        let (task, handles) = new_task_with_handles(TTL);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: status::preconfirmation::success(),
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
        let (task, handles) = new_task_with_handles(TTL);
        let (delegate_secret_key, delegate_public_key) = delegate_key_pair();
        let first_expiration = Tai64(u64::MAX - 200);

        let tx_ids = vec![[3u8; 32].into(), [4u8; 32].into()];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|tx_id| Preconfirmation {
                tx_id,
                status: status::preconfirmation::success(),
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
