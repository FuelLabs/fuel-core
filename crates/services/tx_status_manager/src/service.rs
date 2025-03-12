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
    })
}
