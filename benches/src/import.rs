mod count;
mod pressure_block_importer_port;
mod pressure_consensus_port;
mod pressure_peer_to_peer_port;

pub use count::Count;
use fuel_core_services::{
    SharedMutex,
    StateWatcher,
};
pub use pressure_block_importer_port::PressureBlockImporterPort;
pub use pressure_consensus_port::PressureConsensusPort;
pub use pressure_peer_to_peer_port::PressurePeerToPeerPort;
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::sync::{
    watch::Sender,
    Notify,
};

use fuel_core_sync::{
    import::Import,
    state::State,
    Config,
};

pub type PressureImport =
    Import<PressurePeerToPeerPort, PressureBlockImporterPort, PressureConsensusPort>;

#[derive(Default, Clone, Copy)]
pub struct Durations {
    pub headers: Duration,
    pub consensus: Duration,
    pub transactions: Duration,
    pub executes: Duration,
}

pub fn create_import(
    shared_count: SharedMutex<Count>,
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
    let p2p = Arc::new(PressurePeerToPeerPort::new(
        [input.headers, input.transactions],
        shared_count.clone(),
    ));
    let executor = Arc::new(PressureBlockImporterPort::new(
        input.executes,
        shared_count.clone(),
    ));
    let consensus = Arc::new(PressureConsensusPort::new(
        input.consensus,
        shared_count.clone(),
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
