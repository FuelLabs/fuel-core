use fuel_core_services::{
    SharedMutex,
    StateWatcher,
};
pub use fuel_core_sync::import::test_helpers::SharedCounts;
use fuel_core_sync::{
    import::{
        test_helpers::{
            PressureBlockImporter,
            PressureConsensus,
            PressurePeerToPeer,
        },
        Import,
    },
    state::State,
    Config,
};
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::sync::{
    watch::Sender,
    Notify,
};

pub type PressureImport =
    Import<PressurePeerToPeer, PressureBlockImporter, PressureConsensus>;

#[derive(Default, Clone, Copy)]
pub struct Durations {
    pub headers: Duration,
    pub consensus: Duration,
    pub transactions: Duration,
    pub executes: Duration,
}

pub fn provision_import_test(
    shared_count: SharedCounts,
    shared_state: SharedMutex<State>,
    input: Durations,
    header_batch_size: u32,
    max_header_batch_requests: usize,
    max_get_txns_requests: usize,
) -> (
    PressureImport,
    Sender<fuel_core_services::State>,
    StateWatcher,
) {
    let shared_notify = Arc::new(Notify::new());
    let params = Config {
        max_header_batch_requests,
        header_batch_size,
        max_get_txns_requests,
    };
    let p2p = Arc::new(PressurePeerToPeer::new(
        shared_count.clone(),
        [input.headers, input.transactions],
    ));
    let executor = Arc::new(PressureBlockImporter::new(
        shared_count.clone(),
        input.executes,
    ));
    let consensus = Arc::new(PressureConsensus::new(
        shared_count.clone(),
        input.consensus,
    ));

    let (tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let watcher = shutdown.into();
    let import = Import::new(
        shared_state,
        shared_notify,
        params,
        p2p,
        executor,
        consensus,
    );
    (import, tx, watcher)
}
