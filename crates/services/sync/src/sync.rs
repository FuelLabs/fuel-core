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
use fuel_core_types::fuel_types::BlockHeight;
use futures::stream::StreamExt;
use tokio::sync::Notify;

use crate::state::State;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) enum IncomingHeight {
    Observed(BlockHeight),
    Committed(BlockHeight),
}

pub(crate) struct SyncHeights {
    height_stream: BoxStream<IncomingHeight>,
    state: SharedMutex<State>,
    notify: Arc<Notify>,
}

impl SyncHeights {
    pub(crate) fn new(
        height_stream: BoxStream<BlockHeight>,
        committed_height_stream: BoxStream<BlockHeight>,
        state: SharedMutex<State>,
        notify: Arc<Notify>,
    ) -> Self {
        let height_stream = futures::stream::select(
            height_stream.map(IncomingHeight::Observed),
            committed_height_stream.map(IncomingHeight::Committed),
        )
        .into_boxed();
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
        let state_change = match height {
            IncomingHeight::Committed(height) => {
                self.state.apply(|s| s.commit(*height));
                // A new committed height doesn't represent new work for the import stream.
                false
            }
            IncomingHeight::Observed(height) => self.state.apply(|s| s.observe(*height)),
        };
        if state_change {
            dbg!("notify");
            self.notify.notify_one();
        }
        Some(())
    }

    pub(crate) fn map_stream(
        &mut self,
        f: impl FnOnce(BoxStream<IncomingHeight>) -> BoxStream<IncomingHeight>,
    ) {
        let height_stream = core::mem::replace(
            &mut self.height_stream,
            futures::stream::pending().into_boxed(),
        );
        self.height_stream = f(height_stream);
    }
}
