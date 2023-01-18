//! # Sync task
//! Updates the state from the height stream.

use fuel_core_services::SharedMutex;
use fuel_core_types::blockchain::primitives::BlockHeight;
use futures::{
    stream::StreamExt,
    Stream,
};
use tokio::sync::Notify;

use crate::state::State;

#[cfg(test)]
mod tests;

/// Sync the state from the height stream.
/// This stream never blocks or errors.
pub(crate) async fn sync(
    height_stream: &mut (impl Stream<Item = BlockHeight> + Unpin),
    state: &SharedMutex<State>,
    notify: &Notify,
) -> Option<()> {
    let height = height_stream.next().await?;
    let state_change = state.apply(|s| s.observe(*height));
    if state_change {
        notify.notify_one();
    }
    Some(())
}
