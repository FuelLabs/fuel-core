use fuel_core_poa::{
    ports::GetTime,
    pre_confirmation_signature_service::{
        error::Result as PoAResult,
        trigger::KeyRotationTrigger,
    },
};
use fuel_core_types::tai64::Tai64;
use std::time::Duration;

pub struct TimeBasedTrigger<Time> {
    time: Time,
    next_rotation: Tai64,
    rotation_interval: Duration,
}

impl<Time: GetTime> TimeBasedTrigger<Time> {
    pub fn new(time: Time, rotation_interval: Duration) -> Self {
        let now = time.now();
        let _next_rotation = now.0.saturating_add(rotation_interval.as_secs());
        let next_rotation = Tai64(_next_rotation);
        Self {
            time,
            next_rotation,
            rotation_interval,
        }
    }

    fn get_next_rotation(&self) -> PoAResult<Tai64> {
        Ok(self.next_rotation)
    }

    fn set_next_rotation(&mut self, next_rotation: Tai64) -> PoAResult<()> {
        self.next_rotation = next_rotation;
        Ok(())
    }
}

fn duration_between(t1: Tai64, t2: Tai64) -> Duration {
    let diff = t2.0.saturating_sub(t1.0);
    Duration::from_secs(diff)
}

impl<Time: GetTime + Send> KeyRotationTrigger for TimeBasedTrigger<Time> {
    async fn next_rotation(&mut self) -> PoAResult<()> {
        // create a future that will resolve at the next rotation time
        // if that future resolves, update the next rotation time
        let next_rotation = self.get_next_rotation()?;
        let time_to_wait = duration_between(self.time.now(), next_rotation);
        tracing::debug!(
            "next rotation is {:?} and time to wait is {:?}",
            next_rotation,
            time_to_wait.as_secs()
        );
        tokio::time::interval(time_to_wait).tick().await;

        tracing::debug!(
            "next rotation triggered after waiting {:?}",
            time_to_wait.as_secs()
        );
        let new_next_rotation = self
            .next_rotation
            .0
            .saturating_add(self.rotation_interval.as_secs());
        self.set_next_rotation(Tai64(new_next_rotation))?;
        tracing::debug!("next rotation set to {:?}", self.next_rotation);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
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
        // first rotation
        {
            let mut fut = tokio_test::task::spawn(trigger.next_rotation());
            tokio_test::assert_pending!(fut.poll());
            tokio::time::advance(advance_time_duration).await;
            tokio_test::assert_ready!(fut.poll()).expect("should trigger");
        }

        // when
        let mut fut = tokio_test::task::spawn(trigger.next_rotation());
        tokio_test::assert_pending!(fut.poll());

        // then
        let advance_time = Duration::from_secs(rotation_interval);
        tokio::time::advance(advance_time).await;
        tokio_test::assert_ready!(fut.poll()).expect("should trigger");
    }
}
