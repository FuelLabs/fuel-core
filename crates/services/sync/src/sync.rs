//! # Sync task
//! Updates the state from the height stream.

use std::sync::Arc;

use fuel_core_services::{
    stream::{
        BoxStream,
        IntoBoxStream,
    },
    SharedMutex,
};
use fuel_core_types::blockchain::primitives::BlockHeight;
use futures::stream::StreamExt;
use tokio::sync::Notify;

use crate::state::State;

#[cfg(test)]
mod tests;

pub(crate) struct SyncHeights {
    height_stream: BoxStream<BlockHeight>,
    state: SharedMutex<State>,
    notify: Arc<Notify>,
}

impl SyncHeights {
    pub(crate) fn new(
        height_stream: BoxStream<BlockHeight>,
        state: SharedMutex<State>,
        notify: Arc<Notify>,
    ) -> Self {
        Self {
            height_stream,
            state,
            notify,
        }
    }

    #[tracing::instrument(skip(self))]
    /// Sync the state from the height stream.
    /// This stream never blocks or errors.
    pub(crate) async fn sync(&mut self) -> Option<()> {
        let height = self.height_stream.next().await?;
        let state_change = self.state.apply(|s| s.observe(*height));
        if state_change {
            self.notify.notify_one();
        }
        Some(())
    }

    pub(crate) fn map_stream(
        &mut self,
        f: impl FnOnce(BoxStream<BlockHeight>) -> BoxStream<BlockHeight>,
    ) {
        let height_stream = core::mem::replace(
            &mut self.height_stream,
            futures::stream::pending().into_boxed(),
        );
        self.height_stream = f(height_stream);
    }
}
