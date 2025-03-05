use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    Mappable,
    PredicateStorageRequirements,
    Result as StorageResult,
    StorageInspect,
    StorageRead,
    StorageSize,
};
use fuel_core_types::{
    blockchain::{
        header::ConsensusParametersVersion,
        SealedBlock,
    },
    entities::{
        coins::coin::CompressedCoin,
        relayer::message::Message,
    },
    fuel_tx::{
        BlobId,
        Bytes32,
        ConsensusParameters,
        Contract,
        ContractId,
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    fuel_vm::{
        BlobBytes,
        BlobData,
    },
    services::{
        block_importer::{
            ImportResult,
            SharedImportResult,
        },
        p2p::{
            GossipData,
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            PeerId,
            PreconfirmationMessage,
        },
        txpool::TransactionStatus,
    },
};
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};
use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReceiverStream;

use crate::ports::P2PSubscriptions;

mockall::mock! {
    pub P2P {}

    impl P2PSubscriptions for P2P {
        type GossipedStatuses = GossipData<PreconfirmationMessage>;

        fn gossiped_tx_statuses(&self) -> BoxStream<<MockP2P as P2PSubscriptions>::GossipedStatuses>;
    }

    /*
    impl NotifyP2P for P2P {
        fn notify_gossip_transaction_validity(
            &self,
            message_info: GossipsubMessageInfo,
            validity: GossipsubMessageAcceptance,
        ) -> anyhow::Result<()>;

        fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;
    }

    #[async_trait::async_trait]
    impl P2PRequests for P2P {
        async fn request_tx_ids(&self, peer_id: PeerId) -> anyhow::Result<Vec<TxId>>;

        async fn request_txs(
            &self,
            peer_id: PeerId,
            tx_ids: Vec<TxId>,
        ) -> anyhow::Result<Vec<Option<Transaction>>>;
    }
    */
}

impl MockP2P {
    // TODO[RC]: statuses vs Preconfirmations :-/
    pub fn new_with_statuses(statuses: Vec<PreconfirmationMessage>) -> Self {
        let mut p2p = MockP2P::default();
        p2p.expect_gossiped_tx_statuses().returning(move || {
            let statuses_clone = statuses.clone();
            let stream = fuel_core_services::stream::unfold(
                statuses_clone,
                |mut statuses| async {
                    let maybe_status = statuses.pop();
                    if let Some(status) = maybe_status {
                        Some((GossipData::new(status, vec![], vec![]), statuses))
                    } else {
                        core::future::pending().await
                    }
                },
            );
            Box::pin(stream)
        });

        p2p
    }
}
