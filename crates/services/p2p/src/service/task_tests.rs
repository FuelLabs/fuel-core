#![allow(non_snake_case)]
use crate::ports::P2pDb;

use super::*;

use crate::{
    gossipsub::topics::TX_PRECONFIRMATIONS_GOSSIP_TOPIC,
    peer_manager::heartbeat_data::HeartbeatData,
};
use fuel_core_services::{
    Service,
    State,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::consensus::Genesis,
    fuel_types::BlockHeight,
};
use futures::FutureExt;
use libp2p::gossipsub::TopicHash;
use std::{
    collections::VecDeque,
    time::SystemTime,
};

#[derive(Clone, Debug)]
struct FakeDb;

impl AtomicView for FakeDb {
    type LatestView = Self;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(self.clone())
    }
}

impl P2pDb for FakeDb {
    fn get_sealed_headers(
        &self,
        _block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<SealedBlockHeader>>> {
        unimplemented!()
    }

    fn get_transactions(
        &self,
        _block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>> {
        unimplemented!()
    }

    fn get_genesis(&self) -> StorageResult<Genesis> {
        Ok(Default::default())
    }
}

#[derive(Clone, Debug)]
struct FakeBlockImporter;

impl BlockHeightImporter for FakeBlockImporter {
    fn next_block_height(&self) -> BoxStream<BlockHeight> {
        Box::pin(fuel_core_services::stream::pending())
    }
}

#[derive(Clone, Debug)]
struct FakeTxPool;

impl TxPool for FakeTxPool {
    async fn get_tx_ids(
        &self,
        _max_txs: usize,
    ) -> anyhow::Result<Vec<fuel_core_types::fuel_tx::TxId>> {
        Ok(vec![])
    }

    async fn get_full_txs(
        &self,
        tx_ids: Vec<TxId>,
    ) -> anyhow::Result<Vec<Option<NetworkableTransactionPool>>> {
        Ok(tx_ids.iter().map(|_| None).collect())
    }
}

#[tokio::test]
async fn start_and_stop_awaits_works() {
    let p2p_config = Config::<NotInitialized>::default("start_stop_works");
    let (shared_state, request_receiver) = build_shared_state(p2p_config.clone());
    let service = new_service(
        ChainId::default(),
        0.into(),
        p2p_config,
        shared_state,
        request_receiver,
        FakeDb,
        FakeBlockImporter,
        FakeTxPool,
    );

    // Node with p2p service started
    assert!(service.start_and_await().await.unwrap().started());
    // Node with p2p service stopped
    assert!(service.stop_and_await().await.unwrap().stopped());
}

struct FakeP2PService {
    peer_info: Vec<(PeerId, PeerInfo)>,
    next_event_stream: BoxStream<FuelP2PEvent>,
}

impl TaskP2PService for FakeP2PService {
    fn update_metrics<T>(&self, _: T)
    where
        T: FnOnce(),
    {
        unimplemented!()
    }

    fn get_all_peer_info(&self) -> Vec<(&PeerId, &PeerInfo)> {
        self.peer_info.iter().map(|tup| (&tup.0, &tup.1)).collect()
    }

    fn get_peer_id_with_height(&self, _height: &BlockHeight) -> Option<PeerId> {
        todo!()
    }

    fn next_event(&mut self) -> BoxFuture<'_, Option<FuelP2PEvent>> {
        self.next_event_stream.next().boxed()
    }

    fn publish_message(
        &mut self,
        _message: GossipsubBroadcastRequest,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn send_request_msg(
        &mut self,
        _peer_id: Option<PeerId>,
        _request_msg: RequestMessage,
        _on_response: ResponseSender,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn send_response_msg(
        &mut self,
        _request_id: InboundRequestId,
        _message: V2ResponseMessage,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn report_message(
        &mut self,
        _message: GossipsubMessageInfo,
        _acceptance: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn report_peer(
        &mut self,
        _peer_id: PeerId,
        _score: AppScore,
        _reporting_service: &str,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn update_block_height(&mut self, _height: BlockHeight) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct FakeDB;

impl AtomicView for FakeDB {
    type LatestView = Self;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(self.clone())
    }
}

impl P2pDb for FakeDB {
    fn get_sealed_headers(
        &self,
        _block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<SealedBlockHeader>>> {
        todo!()
    }

    fn get_transactions(
        &self,
        _block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>> {
        todo!()
    }

    fn get_genesis(&self) -> StorageResult<Genesis> {
        todo!()
    }
}

struct FakeBroadcast {
    pub peer_reports: mpsc::Sender<(FuelPeerId, AppScore, String)>,
    pub confirmation_gossip_broadcast: mpsc::Sender<P2PPreConfirmationGossipData>,
}

impl Broadcast for FakeBroadcast {
    fn report_peer(
        &self,
        peer_id: FuelPeerId,
        report: AppScore,
        reporting_service: &'static str,
    ) -> anyhow::Result<()> {
        self.peer_reports
            .try_send((peer_id, report, reporting_service.to_string()))?;
        Ok(())
    }

    fn block_height_broadcast(
        &self,
        _block_height_data: BlockHeightHeartbeatData,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn tx_broadcast(&self, _transaction: TransactionGossipData) -> anyhow::Result<()> {
        todo!()
    }

    fn pre_confirmation_broadcast(
        &self,
        confirmations: P2PPreConfirmationGossipData,
    ) -> anyhow::Result<()> {
        self.confirmation_gossip_broadcast.try_send(confirmations)?;
        Ok(())
    }

    fn new_tx_subscription_broadcast(&self, _peer_id: FuelPeerId) -> anyhow::Result<()> {
        todo!()
    }
}

#[tokio::test]
async fn peer_heartbeat_reputation_checks__slow_heartbeat_sends_reports() {
    // given
    let peer_id = PeerId::random();
    // more than limit
    let last_duration = Duration::from_secs(30);
    let mut durations = VecDeque::new();
    durations.push_front(last_duration);

    let heartbeat_data = HeartbeatData {
        block_height: None,
        last_heartbeat: Instant::now(),
        last_heartbeat_sys: SystemTime::now(),
        window: 0,
        durations,
    };
    let peer_info = PeerInfo {
        peer_addresses: Default::default(),
        client_version: None,
        heartbeat_data,
        score: 100.0,
    };
    let peer_info = vec![(peer_id, peer_info)];
    let p2p_service = FakeP2PService {
        peer_info,
        next_event_stream: Box::pin(futures::stream::pending()),
    };
    let (request_sender, request_receiver) = mpsc::channel(100);

    let (report_sender, mut report_receiver) = mpsc::channel(100);
    let broadcast = FakeBroadcast {
        peer_reports: report_sender,
        confirmation_gossip_broadcast: mpsc::channel(100).0,
    };

    // Less than actual
    let heartbeat_max_avg_interval = Duration::from_secs(20);
    // Greater than actual
    let heartbeat_max_time_since_last = Duration::from_secs(40);

    // Arbitrary values
    let heartbeat_peer_reputation_config = HeartbeatPeerReputationConfig {
        old_heartbeat_penalty: 5.6,
        low_heartbeat_frequency_penalty: 20.45,
    };

    let mut task = Task {
        chain_id: Default::default(),
        response_timeout: Default::default(),
        p2p_service,
        view_provider: FakeDB,
        next_block_height: FakeBlockImporter.next_block_height(),
        tx_pool: FakeTxPool,
        request_receiver,
        request_sender,
        db_heavy_task_processor: SyncProcessor::new("Test", 1, 1).unwrap(),
        tx_pool_heavy_task_processor: AsyncProcessor::new("Test", 1, 1).unwrap(),
        broadcast,
        max_headers_per_request: 0,
        max_txs_per_request: 100,
        heartbeat_check_interval: Duration::from_secs(0),
        heartbeat_max_avg_interval,
        heartbeat_max_time_since_last,
        next_check_time: Instant::now(),
        heartbeat_peer_reputation_config: heartbeat_peer_reputation_config.clone(),
        cached_view: Arc::new(CachedView::new(100, false)),
    };
    let (watch_sender, watch_receiver) = tokio::sync::watch::channel(State::Started);
    let mut watcher = StateWatcher::from(watch_receiver);

    // when
    let (report_peer_id, report, reporting_service) = tokio::time::timeout(
        Duration::from_secs(1),
        wait_until_report_received(&mut report_receiver, &mut task, &mut watcher),
    )
    .await
    .unwrap();

    // then
    watch_sender.send(State::Stopped).unwrap();

    assert_eq!(
        FuelPeerId::from(peer_id.to_bytes().to_vec()),
        report_peer_id
    );
    assert_eq!(
        report,
        heartbeat_peer_reputation_config.low_heartbeat_frequency_penalty
    );
    assert_eq!(reporting_service, "p2p");
}

#[tokio::test]
async fn peer_heartbeat_reputation_checks__old_heartbeat_sends_reports() {
    // given
    let peer_id = PeerId::random();
    // under the limit
    let last_duration = Duration::from_secs(5);
    let last_heartbeat = Instant::now() - Duration::from_secs(50);
    let last_heartbeat_sys = SystemTime::now() - Duration::from_secs(50);
    let mut durations = VecDeque::new();
    durations.push_front(last_duration);

    let heartbeat_data = HeartbeatData {
        block_height: None,
        last_heartbeat,
        last_heartbeat_sys,
        window: 0,
        durations,
    };
    let peer_info = PeerInfo {
        peer_addresses: Default::default(),
        client_version: None,
        heartbeat_data,
        score: 100.0,
    };
    let peer_info = vec![(peer_id, peer_info)];
    let p2p_service = FakeP2PService {
        peer_info,
        next_event_stream: Box::pin(futures::stream::pending()),
    };
    let (request_sender, request_receiver) = mpsc::channel(100);

    let (report_sender, mut report_receiver) = mpsc::channel(100);
    let broadcast = FakeBroadcast {
        peer_reports: report_sender,
        confirmation_gossip_broadcast: mpsc::channel(100).0,
    };

    // Greater than actual
    let heartbeat_max_avg_interval = Duration::from_secs(20);
    // Less than actual
    let heartbeat_max_time_since_last = Duration::from_secs(40);

    // Arbitrary values
    let heartbeat_peer_reputation_config = HeartbeatPeerReputationConfig {
        old_heartbeat_penalty: 5.6,
        low_heartbeat_frequency_penalty: 20.45,
    };

    let mut task = Task {
        chain_id: Default::default(),
        response_timeout: Default::default(),
        p2p_service,
        view_provider: FakeDB,
        tx_pool: FakeTxPool,
        next_block_height: FakeBlockImporter.next_block_height(),
        request_receiver,
        request_sender,
        db_heavy_task_processor: SyncProcessor::new("Test", 1, 1).unwrap(),
        tx_pool_heavy_task_processor: AsyncProcessor::new("Test", 1, 1).unwrap(),
        broadcast,
        max_headers_per_request: 0,
        max_txs_per_request: 100,
        heartbeat_check_interval: Duration::from_secs(0),
        heartbeat_max_avg_interval,
        heartbeat_max_time_since_last,
        next_check_time: Instant::now(),
        heartbeat_peer_reputation_config: heartbeat_peer_reputation_config.clone(),
        cached_view: Arc::new(CachedView::new(100, false)),
    };
    let (watch_sender, watch_receiver) = tokio::sync::watch::channel(State::Started);
    let mut watcher = StateWatcher::from(watch_receiver);

    // when
    // we run this in a loop to ensure that the task is run until it reports
    let (report_peer_id, report, reporting_service) = tokio::time::timeout(
        Duration::from_secs(1),
        wait_until_report_received(&mut report_receiver, &mut task, &mut watcher),
    )
    .await
    .unwrap();

    // then
    watch_sender.send(State::Stopped).unwrap();

    assert_eq!(
        FuelPeerId::from(peer_id.to_bytes().to_vec()),
        report_peer_id
    );
    assert_eq!(
        report,
        heartbeat_peer_reputation_config.old_heartbeat_penalty
    );
    assert_eq!(reporting_service, "p2p");
}

async fn wait_until_report_received(
    report_receiver: &mut Receiver<(FuelPeerId, AppScore, String)>,
    task: &mut Task<FakeP2PService, FakeDB, FakeBroadcast, FakeTxPool>,
    watcher: &mut StateWatcher,
) -> (FuelPeerId, AppScore, String) {
    loop {
        let _ = task.run(watcher).await;
        if let Ok((peer_id, recv_report, service)) = report_receiver.try_recv() {
            return (peer_id, recv_report, service);
        }
    }
}

#[tokio::test]
async fn should_process_all_imported_block_under_infinite_events_from_p2p() {
    // Given
    let (blocks_processed_sender, mut block_processed_receiver) = mpsc::channel(1);
    let next_block_height = Box::pin(futures::stream::repeat_with(move || {
        blocks_processed_sender.try_send(()).unwrap();
        BlockHeight::from(0)
    }));
    let infinite_event_stream = Box::pin(futures::stream::empty());
    let p2p_service = FakeP2PService {
        peer_info: vec![],
        next_event_stream: infinite_event_stream,
    };

    // Initialization
    let (request_sender, request_receiver) = mpsc::channel(100);
    let broadcast = FakeBroadcast {
        peer_reports: mpsc::channel(100).0,
        confirmation_gossip_broadcast: mpsc::channel(100).0,
    };
    let mut task = Task {
        chain_id: Default::default(),
        response_timeout: Default::default(),
        p2p_service,
        tx_pool: FakeTxPool,
        view_provider: FakeDB,
        next_block_height,
        request_receiver,
        request_sender,
        db_heavy_task_processor: SyncProcessor::new("Test", 1, 1).unwrap(),
        tx_pool_heavy_task_processor: AsyncProcessor::new("Test", 1, 1).unwrap(),
        broadcast,
        max_headers_per_request: 0,
        max_txs_per_request: 100,
        heartbeat_check_interval: Duration::from_secs(0),
        heartbeat_max_avg_interval: Default::default(),
        heartbeat_max_time_since_last: Default::default(),
        next_check_time: Instant::now(),
        heartbeat_peer_reputation_config: Default::default(),
        cached_view: Arc::new(CachedView::new(100, false)),
    };
    let mut watcher = StateWatcher::started();
    // End of initialization

    for _ in 0..100 {
        // When
        let _ = task.run(&mut watcher).await;

        // Then
        block_processed_receiver
            .try_recv()
            .expect("Should process the block height even under p2p pressure");
    }
}

fn arb_tx_preconfirmation_gossip_message() -> FuelP2PEvent {
    let peer_id = PeerId::random();
    let message_id = vec![1, 2, 3, 4, 5].into();
    let topic_hash = TopicHash::from_raw(TX_PRECONFIRMATIONS_GOSSIP_TOPIC);
    let preconfirmations = P2PPreConfirmationMessage::default_test_confirmation();
    let message = GossipsubMessage::TxPreConfirmations(preconfirmations);
    FuelP2PEvent::GossipsubMessage {
        peer_id,
        message_id,
        topic_hash,
        message,
    }
}

#[tokio::test]
async fn run__gossip_message_from_p2p_service_is_broadcasted__tx_preconfirmations() {
    // given
    let gossip_message_event = arb_tx_preconfirmation_gossip_message();
    let events = vec![gossip_message_event.clone()];
    let event_stream = futures::stream::iter(events);
    let p2p_service = FakeP2PService {
        peer_info: vec![],
        next_event_stream: Box::pin(event_stream),
    };
    let (preconfirmations_sender, mut preconfirmations_receiver) = mpsc::channel(100);
    let broadcast = FakeBroadcast {
        peer_reports: mpsc::channel(100).0,
        confirmation_gossip_broadcast: preconfirmations_sender,
    };
    let (request_sender, request_receiver) = mpsc::channel(100);
    let mut task = Task {
        chain_id: Default::default(),
        response_timeout: Default::default(),
        p2p_service,
        view_provider: FakeDB,
        next_block_height: FakeBlockImporter.next_block_height(),
        tx_pool: FakeTxPool,
        request_receiver,
        request_sender,
        db_heavy_task_processor: SyncProcessor::new("Test", 1, 1).unwrap(),
        tx_pool_heavy_task_processor: AsyncProcessor::new("Test", 1, 1).unwrap(),
        broadcast,
        max_headers_per_request: 0,
        max_txs_per_request: 100,
        heartbeat_check_interval: Duration::from_secs(0),
        heartbeat_max_avg_interval: Default::default(),
        heartbeat_max_time_since_last: Default::default(),
        next_check_time: Instant::now(),
        heartbeat_peer_reputation_config: Default::default(),
        cached_view: Arc::new(CachedView::new(100, false)),
    };

    // when
    let mut watcher = StateWatcher::started();
    let _ = task.run(&mut watcher).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // then
    let actual = preconfirmations_receiver.try_recv().unwrap().data.unwrap();
    let FuelP2PEvent::GossipsubMessage { message, .. } = gossip_message_event else {
        panic!("Expected GossipsubMessage event");
    };
    let GossipsubMessage::TxPreConfirmations(expected) = message else {
        panic!("Expected Preconfirmations message");
    };
    assert_eq!(expected, actual);
}
