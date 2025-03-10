use crate::ports::{
    P2PPreConfirmationGossipData,
    P2PPreConfirmationMessage,
    P2PSubscriptions,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_types::services::p2p::GossipData;

mockall::mock! {
    pub P2P {}

    impl P2PSubscriptions for P2P {
        type GossipedStatuses = P2PPreConfirmationGossipData;

        fn gossiped_tx_statuses(&self) -> BoxStream<<MockP2P as P2PSubscriptions>::GossipedStatuses>;
    }
}

impl MockP2P {
    pub fn new_with_statuses(statuses: Vec<P2PPreConfirmationMessage>) -> Self {
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
