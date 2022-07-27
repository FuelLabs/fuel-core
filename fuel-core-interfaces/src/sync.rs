use tokio::sync::oneshot;

pub enum SyncStatus {
    Stopped,
    InitialSync,
}

pub enum SyncMpsc {
    Status { ret: oneshot::Sender<SyncStatus> },
    Start,
    Stop,
}
