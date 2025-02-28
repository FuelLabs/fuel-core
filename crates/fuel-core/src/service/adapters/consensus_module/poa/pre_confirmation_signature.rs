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
mod trigger;

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
        tokio::time::sleep(time_to_wait).await;
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
