use fuel_core_poa::pre_confirmation_signature_service::{
    error::Result as PoAResult,
    trigger::KeyRotationTrigger,
};
use std::time::Duration;

#[allow(dead_code)]
pub struct TimeBasedTrigger {
    interval: tokio::time::Interval,
}

impl TimeBasedTrigger {
    pub fn new(rotation_interval: Duration) -> Self {
        Self {
            interval: tokio::time::interval(rotation_interval),
        }
    }
}

impl KeyRotationTrigger for TimeBasedTrigger {
    async fn next_rotation(&mut self) -> PoAResult<()> {
        self.interval.tick().await;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;

    #[tokio::test]
    async fn next_rotation__triggers_at_expected_time() {
        tokio::time::pause();
        // given
        let rotation_interval = 10;
        let rotation_interval_duration = Duration::from_secs(rotation_interval);

        let mut trigger = TimeBasedTrigger::new(rotation_interval_duration);

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

        let mut trigger = TimeBasedTrigger::new(rotation_interval_duration);

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
}
