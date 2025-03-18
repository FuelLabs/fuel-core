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
        TxId,
    },
    services::{
        p2p::{
            GossipData,
            PreConfirmationMessage,
            Sealed,
        },
        preconfirmation::Preconfirmation,
        txpool::TransactionStatus,
    },
};
use futures::StreamExt;
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

pub struct Task {
    manager: TxStatusManager,
    subscriptions: Subscriptions,
    read_requests_receiver: mpsc::Receiver<ReadRequest>,
    write_requests_receiver: mpsc::UnboundedReceiver<WriteRequest>,
    shared_data: SharedData,
}

impl Task {
    // TODO: Implement signatures verifications logic for preconfirmation logic.
    //  https://github.com/FuelLabs/fuel-core/issues/2823
    fn new_preconfirmations_from_p2p(
        &mut self,
        preconfirmations: P2PPreConfirmationMessage,
    ) {
        match preconfirmations {
            PreConfirmationMessage::Delegate(_) => {}
            PreConfirmationMessage::Preconfirmations(sealed) => {
                let Sealed {
                    signature: _,
                    entity,
                } = sealed;
                entity.preconfirmations.into_iter().for_each(
                    |Preconfirmation { tx_id, status }| {
                        let status: TransactionStatus = status.into();
                        self.manager.status_update(tx_id, status);
                    },
                );
            }
        }
    }
}

#[async_trait::async_trait]
impl RunnableService for Task {
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

impl RunnableTask for Task {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                dbg!("1");
                TaskNextAction::Stop
            }

            tx_status_from_p2p = self.subscriptions.new_tx_status.next() => {
                dbg!("2");
                if let Some(GossipData { data, .. }) = tx_status_from_p2p {
                    if let Some(msg) = data {
                        self.new_preconfirmations_from_p2p(msg);
                    }
                    dbg!("2.1");
                    TaskNextAction::Continue
                } else {
                    dbg!("2.2");
                    TaskNextAction::Stop
                }
            }

            request = self.write_requests_receiver.recv() => {
                dbg!("3");
                match request {
                    Some(WriteRequest::UpdateStatus { tx_id, status }) => {
                        dbg!("3.1");
                        self.manager.status_update(tx_id, status);
                        TaskNextAction::Continue
                    }
                    Some(WriteRequest::NotifySkipped { tx_ids_and_reason }) => {
                        dbg!("3.2");
                        self.manager.notify_skipped_txs(tx_ids_and_reason);
                        TaskNextAction::Continue
                    }
                    None => {
                        dbg!("3.3");
                        TaskNextAction::Stop
                    },
                }
            }

            request = self.read_requests_receiver.recv() => {
                dbg!("4");
                match request {
                    Some(ReadRequest::GetStatus { tx_id, sender }) => {
                        dbg!("4.1");
                        let status = self.manager.status(&tx_id);
                        let _ = sender.send(status.cloned());
                        TaskNextAction::Continue
                    }
                    Some(ReadRequest::Subscribe { tx_id, sender }) => {
                        dbg!("4.2");
                        let result = self.manager.tx_update_subscribe(tx_id);
                        let _ = sender.send(result);
                        TaskNextAction::Continue
                    }
                    None => {
                        dbg!("4.3");
                        TaskNextAction::Stop
                    },
                }
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        panic!();
        dbg!("5");
        Ok(())
    }
}

pub fn new_service<P2P>(p2p: P2P, config: Config) -> ServiceRunner<Task>
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
        TxStatusManager::new(tx_status_sender, config.status_cache_ttl, config.metrics);

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
    })
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use std::time::Duration;

    use fuel_core_services::{
        Service,
        ServiceRunner,
    };
    use fuel_core_types::{
        fuel_tx::{
            Bytes32,
            Bytes64,
        },
        services::{
            p2p::{
                GossipData,
                Sealed,
            },
            preconfirmation::{
                Preconfirmation,
                PreconfirmationStatus,
                Preconfirmations,
            },
            txpool::TransactionStatus,
        },
        tai64::Tai64,
    };
    use futures::StreamExt;
    use tokio::sync::{
        mpsc,
        oneshot,
    };
    use tokio_stream::wrappers::ReceiverStream;

    use crate::{
        manager::TxStatusManager,
        ports::P2PPreConfirmationMessage,
        subscriptions::Subscriptions,
        update_sender::TxStatusChange,
    };

    use super::{
        ReadRequest,
        SharedData,
        Task,
        WriteRequest,
    };
    use fuel_core_types::ed25519_dalek::{
        Signer,
        SigningKey as DalekSigningKey,
        VerifyingKey as DalekVerifyingKey,
    };

    const TTL: Duration = Duration::from_secs(4);

    struct Handles {
        pub subscriptions_sender: mpsc::Sender<GossipData<P2PPreConfirmationMessage>>,
        pub write_requests_sender: mpsc::UnboundedSender<WriteRequest>,
        pub read_requests_sender: mpsc::Sender<ReadRequest>,
    }

    pub(super) mod status {
        pub(super) mod preconfirmation {
            use fuel_core_types::services::preconfirmation::PreconfirmationStatus;

            pub fn success() -> PreconfirmationStatus {
                PreconfirmationStatus::Success {
                    tx_pointer: Default::default(),
                    total_gas: Default::default(),
                    total_fee: Default::default(),
                    receipts: Default::default(),
                    outputs: Default::default(),
                }
            }

            pub fn squeezed_out() -> PreconfirmationStatus {
                PreconfirmationStatus::SqueezedOut {
                    reason: "fishy transaction".to_string(),
                }
            }

            pub fn failure() -> PreconfirmationStatus {
                PreconfirmationStatus::Failure {
                    tx_pointer: Default::default(),
                    total_gas: Default::default(),
                    total_fee: Default::default(),
                    receipts: Default::default(),
                    outputs: Default::default(),
                }
            }
        }

        pub(super) mod transaction {
            use fuel_core_types::{
                fuel_crypto::rand::{
                    rngs::StdRng,
                    seq::SliceRandom,
                },
                services::txpool::TransactionStatus,
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
                TransactionStatus::PreConfirmationSuccess(Default::default())
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
                TransactionStatus::PreConfirmationFailure(Default::default())
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

    fn new_task_with_handles() -> (Task, Handles) {
        let (read_requests_sender, read_requests_receiver) = mpsc::channel(1);
        let (write_requests_sender, write_requests_receiver) = mpsc::unbounded_channel();
        let shared_data = SharedData {
            read_requests_sender: read_requests_sender.clone(),
            write_requests_sender: write_requests_sender.clone(),
        };

        //
        //
        // let _updater_sender = tx_status_change.update_sender.clone();

        let (sender, receiver) = mpsc::channel(1_000);
        let new_tx_status = Box::pin(ReceiverStream::new(receiver));
        let subscriptions = Subscriptions { new_tx_status };
        let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
        let manager = TxStatusManager::new(tx_status_change, TTL, false);

        let handles = Handles {
            subscriptions_sender: sender,
            write_requests_sender,
            read_requests_sender,
        };

        let task = Task {
            subscriptions,
            manager,
            read_requests_receiver,
            write_requests_receiver,
            shared_data,
        };

        (task, handles)
    }

    // TODO[RC]: Move to utils, since this is shared between multiple modules
    fn delegate_key_pair() -> (DalekSigningKey, DalekVerifyingKey) {
        let secret_key = [99u8; 32];
        let secret_key = DalekSigningKey::from_bytes(&secret_key);
        let public_key = secret_key.verifying_key();
        (secret_key, public_key)
    }

    /*
    P2P preparation

    #[tokio::test]
    async fn status_management__can_register_preconfirmation_messages() {
        let (task, handles) = new_task_with_handles();

        let (delegate_signing_key, _delegate_verifying_key) = delegate_key_pair();
        let expiration = Tai64(u64::MAX);

        // Given
        let tx1_id = [1u8; 32].into();
        let tx2_id = [2u8; 32].into();
        let tx3_id = [3u8; 32].into();
        let tx_ids = vec![
            (tx1_id, success()),
            (tx2_id, squeezed_out()),
            (tx3_id, failure()),
        ];
        let preconfirmations = tx_ids
            .clone()
            .into_iter()
            .map(|(tx_id, status)| Preconfirmation { tx_id, status })
            .collect();

        let entity = Preconfirmations {
            expiration,
            preconfirmations,
        };
        let bytes = postcard::to_allocvec(&entity).unwrap();
        let typed_signature = delegate_signing_key.sign(&bytes);
        let signature = Bytes64::new(typed_signature.to_bytes());
        let sealed = Sealed { entity, signature };
        let inner = P2PPreConfirmationMessage::Preconfirmations(sealed);

        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // When
        handles.write_requests_sender.send(inner).unwrap();

        // Then
        // TODO: Also check statuses themselves, not only presence
        // for (tx_id, _) in tx_ids {
        //     assert!(
        //         task.manager.status(&tx_id).is_some(),
        //         "tx_id {:?} should be present",
        //         tx_id
        //     );
        // }

        // Also check notifications...
    }
    */

    #[tokio::test]
    async fn status_management__can_register_preconfirmation_messages() {
        let (task, handles) = new_task_with_handles();
        let service = ServiceRunner::new(task);
        service.start_and_await().await.unwrap();

        // Given
        let tx1_id: Bytes32 = [1u8; 32].into();
        let tx2_id: Bytes32 = [2u8; 32].into();
        let tx3_id: Bytes32 = [3u8; 32].into();
        let tx4_id: Bytes32 = [4u8; 32].into();
        let tx5_id: Bytes32 = [5u8; 32].into();
        let tx6_id: Bytes32 = [6u8; 32].into();
        let tx7_id: Bytes32 = [7u8; 32].into();
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
        status_updates.iter().for_each(|(tx_id, status)| {
            handles
                .write_requests_sender
                .send(WriteRequest::UpdateStatus {
                    tx_id: (*tx_id).into(),
                    status: status.clone(),
                })
                .unwrap();
        });

        assert_presence_with_status(
            handles.read_requests_sender,
            vec![(tx1_id, status::transaction::submitted())],
        )
        .await;

        // let msg = receiver.await;
        // println!("Received status response: {:?}", msg);

        // let res = service.stop_and_await().await.unwrap();
        // println!("Service stopped: {:?}", res);
        // dbg!(&res);

        // Then
        // TODO: Also check statuses themselves, not only presence
        // for (tx_id, _) in tx_ids {
        //     assert!(
        //         task.manager.status(&tx_id).is_some(),
        //         "tx_id {:?} should be present",
        //         tx_id
        //     );
        // }

        // Also check notifications...
    }

    async fn assert_presence_with_status(
        status_read: mpsc::Sender<ReadRequest>,
        txs: Vec<(Bytes32, TransactionStatus)>,
    ) {
        for (id, status) in txs.iter() {
            let (sender, receiver) = oneshot::channel();
            status_read
                .send(ReadRequest::GetStatus {
                    tx_id: (*id).into(),
                    sender,
                })
                .await
                .unwrap();

            let response = receiver.await.unwrap();
            assert_eq!(response, Some(status.clone()));
        }
    }
}
