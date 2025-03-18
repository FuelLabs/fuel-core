use crate::tests::PublicKey;
use std::sync::Arc;

use fuel_core_services::ServiceRunner;
use fuel_core_types::fuel_crypto::rand::{
    rngs::StdRng,
    SeedableRng,
};
use parking_lot::Mutex;
use std::time::Duration;

use super::mocks::MockP2P;
use crate::{
    config::Config,
    manager::TxStatusManager,
    new_service,
    update_sender::TxStatusChange,
    Task,
};

const TX_STATUS_MANAGER_TTL: Duration = Duration::from_secs(5);

// TODO[RC]: Remove "dead code" when the universe is fully implemented to support new tests
// in `tests_service.rs`
#[allow(dead_code)]
pub struct TestTxStatusManagerUniverse {
    rng: StdRng,
    config: Config,
    tx_status_manager: Option<Arc<Mutex<TxStatusManager>>>,
}

impl Default for TestTxStatusManagerUniverse {
    fn default() -> Self {
        Self {
            rng: StdRng::seed_from_u64(0),
            config: Default::default(),
            tx_status_manager: None,
        }
    }
}

// TODO[RC]: Remove "dead code" when the universe is fully implemented to support new tests
// in `tests_service.rs`
#[allow(dead_code)]
impl TestTxStatusManagerUniverse {
    pub fn config(self, config: Config) -> Self {
        if self.tx_status_manager.is_some() {
            panic!("TxStatusManager already built");
        }
        Self { config, ..self }
    }

    pub fn build_tx_status_manager(&mut self) {
        let tx_status_sender = TxStatusChange::new(1000, Duration::from_secs(360));

        let tx_status_manager = Arc::new(Mutex::new(TxStatusManager::new(
            tx_status_sender,
            TX_STATUS_MANAGER_TTL,
            false,
        )));
        self.tx_status_manager = Some(tx_status_manager.clone());
    }

    pub fn build_service(&self, p2p: Option<MockP2P>) -> ServiceRunner<Task<PublicKey>> {
        let p2p = p2p.unwrap_or_else(|| MockP2P::new_with_statuses(vec![]));
        let arb_pubkey = Default::default();

        new_service(p2p, self.config.clone(), arb_pubkey)
    }
}
