use fuel_core_poa::{
    ports::GetTime,
    pre_confirmation_signature_service::{
        error::Result as PoAResult,
        trigger::KeyRotationTrigger,
    },
};
use fuel_core_types::tai64::Tai64;
use std::time::Duration;

#[allow(dead_code)]
pub struct TimeBasedTrigger<T> {
    time: T,
    interval: tokio::time::Interval,
    expiration: Duration,
}

impl<T> TimeBasedTrigger<T> {
    pub fn new(time: T, rotation_interval: Duration, expiration: Duration) -> Self {
        let mut interval = tokio::time::interval(rotation_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Self {
            time,
            interval,
            expiration,
        }
    }
}

impl<T: GetTime> KeyRotationTrigger for TimeBasedTrigger<T> {
    async fn next_rotation(&mut self) -> PoAResult<Tai64> {
        let _ = self.interval.tick().await;
        let expiration_raw = self.time.now().0.saturating_add(self.expiration.as_secs());
        let expiration = Tai64(expiration_raw);
        Ok(expiration)
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use super::*;

    struct FakeTime {
        now: Tai64,
    }

    impl FakeTime {
        fn new(now: Tai64) -> Self {
            Self { now }
        }
    }

    impl GetTime for FakeTime {
        fn now(&self) -> Tai64 {
            self.now
        }
    }

    #[tokio::test]
    async fn next_rotation__triggers_at_expected_time() {
        tokio::time::pause();
        // given
        let rotation_interval = 10;
        let rotation_interval_duration = Duration::from_secs(rotation_interval);

        let time = FakeTime::new(Tai64::now());
        let expiration = Duration::from_secs(rotation_interval * 2);
        let mut trigger =
            TimeBasedTrigger::new(time, rotation_interval_duration, expiration);

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

        let time = FakeTime::new(Tai64::now());
        let expiration = Duration::from_secs(rotation_interval * 2);

        let mut trigger =
            TimeBasedTrigger::new(time, rotation_interval_duration, expiration);

        tokio::time::advance(Duration::from_secs(1)).await;
        trigger.next_rotation().await.unwrap();

        // when
        let mut fut = tokio_test::task::spawn(trigger.next_rotation());
        tokio_test::assert_pending!(fut.poll());

        // then
        let advance_time = Duration::from_secs(rotation_interval);
        tokio::time::advance(advance_time).await;
        tokio_test::assert_ready!(fut.poll()).expect("should trigger");
    }

    #[tokio::test]
    async fn next_rotation__returns_expected_expiration_date() {
        tokio::time::pause();
        // given
        let rotation_interval = 10;
        let rotation_interval_duration = Duration::from_secs(rotation_interval);

        let now = Tai64::UNIX_EPOCH;
        let time = FakeTime::new(now);
        let expiration = Duration::from_secs(rotation_interval * 2);
        let mut trigger =
            TimeBasedTrigger::new(time, rotation_interval_duration, expiration);

        // when
        let fut = tokio_test::task::spawn(trigger.next_rotation());

        // then
        let advance_time = Duration::from_secs(rotation_interval + 1);
        tokio::time::advance(advance_time).await;
        let result = fut.await.expect("should trigger");

        assert_eq!(result, now + expiration.as_secs());
    }
}
