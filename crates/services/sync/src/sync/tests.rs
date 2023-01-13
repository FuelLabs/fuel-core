use futures::stream;

use super::*;

#[tokio::test]
async fn test_sync() {
    let height_stream =
        stream::iter([1u32, 2, 3, 4, 5].into_iter().map(BlockHeight::from)).boxed();
    let state = SharedMutex::new(State::new(None, None));
    let notify = Arc::new(Notify::new());

    sync(height_stream, state.clone(), notify).await;

    assert_eq!(state.apply(|s| s.proposed_height().copied()), Some(5u32));
}
