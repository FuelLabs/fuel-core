#![allow(non_snake_case)]
use super::*;
use tokio::time::Instant;

#[derive(Clone)]
struct TokioTime {
    start: Instant,
}

impl TokioTime {
    pub fn new() -> Self {
        let start = Instant::now();
        Self { start }
    }
}

impl GetTime for TokioTime {
    fn now(&self) -> Tai64 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.start);
        Tai64(elapsed.as_secs())
    }
}

#[tokio::test]
async fn next_rotation__triggers_at_expected_time() {
    tokio::time::pause();
    // given
    let rotation_interval = 10;
    let rotation_interval_duration = Duration::from_secs(rotation_interval);
    let fake_time = TokioTime::new();

    let mut trigger = TimeBasedTrigger::new(fake_time, rotation_interval_duration);

    // when
    let mut fut = tokio_test::task::spawn(trigger.next_rotation());
    tokio_test::assert_pending!(fut.poll());

    // then
    let advance_time = Duration::from_secs(rotation_interval + 1);
    tokio::time::advance(advance_time).await;
    tokio_test::assert_ready!(fut.poll()).expect("should trigger");
}

#[tokio::test]
async fn next_rotation__subsequent_triggers_are_same_interval() {
    tokio::time::pause();
    // given
    let rotation_interval = 10;
    let rotation_interval_duration = Duration::from_secs(rotation_interval);
    let fake_time = TokioTime::new();

    let mut trigger = TimeBasedTrigger::new(fake_time, rotation_interval_duration);

    let advance_time = rotation_interval + 1;
    let advance_time_duration = Duration::from_secs(advance_time);
    tokio::time::advance(advance_time_duration).await;
    trigger.next_rotation().await.unwrap();

    // when
    let mut fut = tokio_test::task::spawn(trigger.next_rotation());
    tokio_test::assert_pending!(fut.poll());

    // then
    let advance_time = Duration::from_secs(rotation_interval);
    tokio::time::advance(advance_time).await;
    tokio_test::assert_ready!(fut.poll()).expect("should trigger");
}
