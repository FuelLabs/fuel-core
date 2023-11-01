#![allow(dead_code)]

use tokio::{
    sync::mpsc::{
        channel,
        Receiver,
        Sender,
    },
    task::JoinHandle,
    time::{
        Duration,
        Instant,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnConflict {
    /// Ignores the new write
    Ignore,
    /// Replaces the previous deadline
    Overwrite,
    /// Picks the earlier time
    Min,
    /// Picks the later time
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlMessage {
    /// Set the deadline
    Set {
        deadline: Instant,
        on_conflict: OnConflict,
    },
    /// Stop the clock, clearing any deadlines
    Clear,
}

async fn deadline_clock_task(
    event_tx: Sender<Instant>,
    mut control_rx: Receiver<ControlMessage>,
) {
    let mut active = false;

    let sleep = tokio::time::sleep(Duration::new(0, 0));
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            ctrl = control_rx.recv() => {
                match ctrl {
                    Some(ControlMessage::Set {deadline, on_conflict}) => {
                        if !active {
                            sleep.as_mut().reset(deadline);
                        } else {
                            match on_conflict {
                                OnConflict::Ignore => {},
                                OnConflict::Overwrite => {
                                    sleep.as_mut().reset(deadline);
                                },
                                OnConflict::Min => {
                                    let new_deadline = sleep.deadline().min(deadline);
                                    sleep.as_mut().reset(new_deadline);
                                },
                                OnConflict::Max => {
                                    let new_deadline = sleep.deadline().max(deadline);
                                    sleep.as_mut().reset(new_deadline);
                                },
                            }
                        }
                        active = true;
                    }
                    Some(ControlMessage::Clear) => {
                        // disable sleep timer
                        active = false;
                    }
                    None => break
                }
            }
            _ = &mut sleep, if active => {
                // Trigger
                active = false;
                if event_tx.send(sleep.deadline()).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// A configurable deadline-mode clock that produces event to
/// an associated channel when the timeout expires.
pub struct DeadlineClock {
    /// Invariant: bounded, limit = 1
    event: Receiver<Instant>,
    /// Invariant: bounded, limit = 1
    control: Sender<ControlMessage>,
    // This field must be defined after `control`, as closing it ends the clock task
    _handle: JoinHandle<()>,
}
impl DeadlineClock {
    pub fn new() -> Self {
        let (event_tx, event_rx) = channel(1);
        let (control_tx, control_rx) = channel(1);
        let handle = tokio::spawn(deadline_clock_task(event_tx, control_rx));
        Self {
            event: event_rx,
            control: control_tx,
            _handle: handle,
        }
    }

    /// Waits until the timeout expires. Sleeps forever when not timeout is set.
    pub async fn wait(&mut self) -> Instant {
        self.event
            .recv()
            .await
            .expect("Deadline clock task has panicked")
    }

    /// Sets the timeout, optionally overwriting the existing value
    pub async fn set_deadline(&self, deadline: Instant, on_conflict: OnConflict) {
        self.control
            .send(ControlMessage::Set {
                deadline,
                on_conflict,
            })
            .await
            .expect("Deadline clock task has panicked");
    }

    /// Sets the timeout, optionally overwriting the existing value
    pub async fn set_timeout(&self, after: Duration, on_conflict: OnConflict) {
        self.set_deadline(
            Instant::now()
                .checked_add(after)
                .expect("Setting timeout after many years doesn't make a lot of sense"),
            on_conflict,
        )
        .await;
    }

    /// Clears the timeout, so that now event is produced when it expires.
    /// If the event has alread occurred, it will not be removed.
    pub async fn clear(&self) {
        self.control
            .send(ControlMessage::Clear)
            .await
            .expect("Deadline clock task has panicked");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        sync::mpsc,
        time::{
            self,
            timeout,
            Duration,
        },
    };

    #[tokio::test]
    async fn deadline_clock_realtime() {
        let mut c = DeadlineClock::new();

        c.set_timeout(Duration::from_millis(10), OnConflict::Overwrite)
            .await;
        timeout(Duration::from_millis(20), c.wait())
            .await
            .expect("Timed out");

        c.set_timeout(Duration::from_millis(10), OnConflict::Overwrite)
            .await;
        c.clear().await;
        timeout(Duration::from_millis(20), c.wait())
            .await
            .expect_err("Completed unexpectedly");

        c.set_timeout(Duration::from_millis(10), OnConflict::Overwrite)
            .await;
        c.set_timeout(Duration::from_millis(100), OnConflict::Overwrite)
            .await;
        timeout(Duration::from_millis(20), c.wait())
            .await
            .expect_err("Completed unexpectedly");
        timeout(Duration::from_millis(100), c.wait())
            .await
            .expect("Timed out");
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_clock_mocktime_expiration() {
        let mut c = DeadlineClock::new();

        c.set_timeout(Duration::from_millis(10), OnConflict::Overwrite)
            .await;

        // Must not expire immediately
        assert_eq!(c.event.try_recv(), Err(mpsc::error::TryRecvError::Empty));

        // Must not expire too soon
        time::sleep(Duration::from_millis(5)).await;
        assert_eq!(c.event.try_recv(), Err(mpsc::error::TryRecvError::Empty));

        // Must expire too soon
        time::sleep(Duration::from_millis(10)).await;
        assert!(c.event.try_recv().is_ok(),);
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_clock_setting_deadline_to_past_triggers_it() {
        let mut c = DeadlineClock::new();

        let in_past1 = Instant::now() - Duration::from_millis(200);
        let in_past2 = Instant::now() - Duration::from_millis(100);

        // Must expire in any short amount of time
        c.set_deadline(in_past1, OnConflict::Overwrite).await;
        time::sleep(Duration::from_millis(1)).await;
        assert!(c.event.try_recv().is_ok(),);

        // Must expire in any short amount of time
        c.set_deadline(in_past2, OnConflict::Overwrite).await;
        time::sleep(Duration::from_millis(1)).await;
        assert!(c.event.try_recv().is_ok(),);
    }
}
