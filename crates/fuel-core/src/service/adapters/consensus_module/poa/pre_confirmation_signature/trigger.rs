use fuel_core_poa::{
    ports::GetTime,
    pre_confirmation_signature_service::{
        error::Result as PoAResult,
        trigger::KeyRotationTrigger,
    },
};
use std::time::Duration;

#[allow(dead_code)]
pub struct TimeBasedTrigger<Time> {
    time: Time,
    // next_rotation: Tai64,
    // rotation_interval: Duration,
    interval: tokio::time::Interval,
}

impl<Time: GetTime> TimeBasedTrigger<Time> {
    pub fn new(time: Time, rotation_interval: Duration) -> Self {
        Self {
            time,
            interval: tokio::time::interval(rotation_interval),
        }
    }
}

impl<Time: GetTime + Send> KeyRotationTrigger for TimeBasedTrigger<Time> {
    async fn next_rotation(&mut self) -> PoAResult<()> {
        self.interval.tick().await;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use fuel_core_types::tai64::Tai64;

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

        tokio::time::advance(Duration::from_secs(1)).await;
        let _first_rotation = trigger.next_rotation().await.unwrap();

        // when
        let mut fut = tokio_test::task::spawn(trigger.next_rotation());
        tokio_test::assert_pending!(fut.poll());

        // then
        let advance_time = Duration::from_secs(rotation_interval);
        tokio::time::advance(advance_time).await;
        tokio_test::assert_ready!(fut.poll()).expect("should trigger");
    }
}
