use futures::stream;

use super::*;

#[tokio::test]
async fn test_sync() {
    let height_stream =
        stream::iter([1u32, 2, 3, 4, 5].into_iter().map(BlockHeight::from)).boxed();
    let state = SharedMutex::new(State::default());
    let notify = Arc::new(Notify::new());

    sync(height_stream, state.clone(), notify).await;

    assert_eq!(
        state.apply(|s| s.best_seen_height),
        Some(BlockHeight::from(5u32))
    );
}
