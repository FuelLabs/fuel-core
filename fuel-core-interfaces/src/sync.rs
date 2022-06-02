use tokio::sync::oneshot;

pub enum SyncStatus {
    Stoped,
    InitialSync,
}

pub enum SyncMpsc {
    Status { ret: oneshot::Sender<SyncStatus> },
    Start,
    Stop,
}
