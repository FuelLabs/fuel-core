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

pub(crate) fn spawn_sync(
    height_stream: BoxStream<'static, BlockHeight>,
    state: SharedMutex<State>,
    notify: Arc<Notify>,
    shutdown: Shutdown,
) -> tokio::task::JoinHandle<()> {
    let height_stream = height_stream.take_until(async move {
        shutdown.wait().await;
    });
    tokio::spawn(async move { sync(height_stream, state, notify).await })
}

async fn sync(
    height_stream: impl Stream<Item = BlockHeight>,
    state: SharedMutex<State>,
    notify: Arc<Notify>,
) {
    height_stream
        .for_each(|h| {
            let state_change = state.apply(|s| s.see(*h));
            if state_change {
                notify.notify_one();
            }
            futures::future::ready(())
        })
        .await;
}
