use fuel_core_poa::{
    ports::GetTime,
    pre_confirmation_signature_service::{
        error::Result as PoAResult,
        trigger::KeyRotationTrigger,
    },
};
use fuel_core_types::tai64::Tai64;
use std::time::Duration;

#[cfg(test)]
mod trigger {
    #![allow(non_snake_case)]
    use super::*;

    struct FakeTime {
        now: Tai64,
    }

    impl FakeTime {
        fn new(now: u64) -> Self {
            let now = Tai64(now);
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
        let fake_time = FakeTime::new(50);

        let mut trigger = TimeBasedTrigger::new(fake_time, rotation_interval_duration);

        // when
        let mut fut = tokio_test::task::spawn(trigger.next_rotation());
        tokio_test::assert_pending!(fut.poll());

        // then
        let advance_time = Duration::from_secs(rotation_interval + 1);
        tokio::time::advance(advance_time).await;
        tokio_test::assert_ready!(fut.poll()).expect("should trigger");
    }
}

pub struct TimeBasedTrigger<Time> {
    time: Time,
    // next_rotation: Mutex<Tai64>,
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
        tokio::time::sleep(time_to_wait).await;
        let new_next_rotation = self
            .time
            .now()
            .0
            .saturating_add(self.rotation_interval.as_secs());
        self.set_next_rotation(Tai64(new_next_rotation))?;
        Ok(())
    }
}
