use std::sync::Arc;

use fuel_core_services::stream::IntoBoxStream;
use futures::stream;

use super::*;

#[tokio::test]
async fn test_sync() {
    let height_stream =
        stream::iter([1u32, 2, 3, 4, 5].into_iter().map(BlockHeight::from))
            .map(IncomingHeight::Observed)
            .into_boxed();
    let state = SharedMutex::new(State::new(None, None));
    let notify = Arc::new(Notify::new());

    let mut s = SyncHeights {
        height_stream,
        state,
        notify,
    };

    while s.sync().await.is_some() {}

    assert_eq!(s.state.apply(|s| s.proposed_height().copied()), Some(5u32));
}
