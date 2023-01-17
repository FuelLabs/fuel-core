//! # Sync task
//! Updates the state from the height stream.
use std::sync::Arc;

use fuel_core_services::{
    SharedMutex,
    Shutdown,
};
use fuel_core_types::blockchain::primitives::BlockHeight;
use futures::{
    stream::{
        BoxStream,
        StreamExt,
    },
    Stream,
};
use tokio::sync::Notify;

use crate::state::State;

#[cfg(test)]
mod tests;

/// Spawn the sync task.
pub(crate) fn spawn_sync(
    height_stream: BoxStream<'static, BlockHeight>,
    state: SharedMutex<State>,
    notify: Arc<Notify>,
    shutdown: Shutdown,
) -> tokio::task::JoinHandle<()> {
    // Stop the sync task when the shutdown signal is received.
    let height_stream = height_stream.take_until(async move {
        shutdown.wait().await;
    });
    tokio::spawn(async move { sync(height_stream, state, notify).await })
}

/// Sync the state from the height stream.
/// This stream never blocks or errors.
async fn sync(
    height_stream: impl Stream<Item = BlockHeight>,
    state: SharedMutex<State>,
    notify: Arc<Notify>,
) {
    height_stream
        .for_each(|h| {
            let state_change = state.apply(|s| s.observe(*h));
            if state_change {
                notify.notify_one();
            }
            futures::future::ready(())
        })
        .await;
}
