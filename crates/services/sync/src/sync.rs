use std::sync::Arc;

use fuel_core_services::SharedMutex;
use fuel_core_types::blockchain::primitives::BlockHeight;
use futures::stream::{
    BoxStream,
    StreamExt,
};
use tokio::sync::Notify;

use crate::State;

#[cfg(test)]
mod tests;

async fn sync(
    height_stream: BoxStream<'static, BlockHeight>,
    state: SharedMutex<State>,
    notify: Arc<Notify>,
) {
    height_stream
        .for_each(|h| {
            let state_change = state.apply(|s| match s.best_seen_height.as_mut() {
                Some(b) => {
                    let s = h > *b;
                    *b = h.max(*b);
                    s
                }
                None => {
                    s.best_seen_height = Some(h);
                    true
                }
            });
            if state_change {
                notify.notify_one();
            }
            futures::future::ready(())
        })
        .await;
}
