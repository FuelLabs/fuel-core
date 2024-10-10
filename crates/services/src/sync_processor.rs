use fuel_core_metrics::futures::{
    metered_future::MeteredFuture,
    FuturesMetrics,
};
use std::sync::Arc;
use tokio::sync::{
    OwnedSemaphorePermit,
    Semaphore,
};

/// A processor that can execute sync tasks with a limit on the number of tasks that can
/// wait in the queue. The number of threads defines how many tasks can be executed in parallel.
pub struct SyncProcessor {
    metric: FuturesMetrics,
    rayon_thread_pool: Option<rayon::ThreadPool>,
    semaphore: Arc<Semaphore>,
}

/// A reservation for a task to be executed by the `SyncProcessor`.
pub struct SyncReservation(OwnedSemaphorePermit);

/// Out of capacity error.
#[derive(Debug, PartialEq, Eq)]
pub struct OutOfCapacity;

impl SyncProcessor {
    /// Create a new `SyncProcessor` with the given number of threads and the number of pending
    /// tasks.
    pub fn new(
        metric_name: &str,
        number_of_threads: usize,
        number_of_pending_tasks: usize,
    ) -> anyhow::Result<Self> {
        let rayon_thread_pool = if number_of_threads != 0 {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(number_of_threads)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to create a rayon pool: {}", e))?;
            Some(pool)
        } else {
            None
        };
        let semaphore = Arc::new(Semaphore::new(number_of_pending_tasks));
        let metric = FuturesMetrics::obtain_futures_metrics(metric_name);
        Ok(Self {
            metric,
            rayon_thread_pool,
            semaphore,
        })
    }

    /// Reserve a slot for a task to be executed.
    pub fn reserve(&self) -> Result<SyncReservation, OutOfCapacity> {
        let permit = self.semaphore.clone().try_acquire_owned();
        if let Ok(permit) = permit {
            Ok(SyncReservation(permit))
        } else {
            Err(OutOfCapacity)
        }
    }

    /// Spawn a task with a reservation.
    pub fn spawn_reserved<F>(&self, reservation: SyncReservation, op: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let permit = reservation.0;
        let sync_future = async move {
            // When task started its works we can free the space.
            drop(permit);
            op()
        };
        let metered_future = MeteredFuture::new(sync_future, self.metric.clone());
        if let Some(rayon_thread_pool) = &self.rayon_thread_pool {
            rayon_thread_pool
                .spawn_fifo(move || futures::executor::block_on(metered_future));
        } else {
            futures::executor::block_on(metered_future)
        }
    }

    /// Try to spawn a task.
    pub fn try_spawn<F>(&self, future: F) -> Result<(), OutOfCapacity>
    where
        F: FnOnce() + Send + 'static,
    {
        let reservation = self.reserve()?;
        self.spawn_reserved(reservation, future);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison)]
mod tests {
    use super::*;
    use std::{
        thread::sleep,
        time::Duration,
    };
    use tokio::time::Instant;

    #[tokio::test]
    async fn one_spawn_single_tasks_works() {
        // Given
        let number_of_pending_tasks = 1;
        let heavy_task_processor =
            SyncProcessor::new("Test", 1, number_of_pending_tasks).unwrap();

        // When
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let result = heavy_task_processor.try_spawn(move || {
            sender.send(()).unwrap();
        });

        // Then
        assert_eq!(result, Ok(()));
        let timeout = tokio::time::timeout(Duration::from_secs(1), receiver).await;
        timeout
            .expect("Shouldn't timeout")
            .expect("Should receive a message");
    }

    #[tokio::test]
    async fn second_spawn_fails_when_limit_is_one_and_first_in_progress() {
        // Given
        let number_of_pending_tasks = 1;
        let heavy_task_processor =
            SyncProcessor::new("Test", 1, number_of_pending_tasks).unwrap();
        let first_spawn_result = heavy_task_processor.try_spawn(move || {
            sleep(Duration::from_secs(1));
        });
        assert_eq!(first_spawn_result, Ok(()));

        // When
        let second_spawn_result = heavy_task_processor.try_spawn(move || {
            sleep(Duration::from_secs(1));
        });

        // Then
        assert_eq!(second_spawn_result, Err(OutOfCapacity));
    }

    #[tokio::test]
    async fn second_spawn_works_when_first_is_finished() {
        let number_of_pending_tasks = 1;
        let heavy_task_processor =
            SyncProcessor::new("Test", 1, number_of_pending_tasks).unwrap();

        // Given
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let first_spawn = heavy_task_processor.try_spawn(move || {
            sleep(Duration::from_secs(1));
            sender.send(()).unwrap();
        });
        assert_eq!(first_spawn, Ok(()));
        receiver.await.unwrap();

        // When
        let second_spawn = heavy_task_processor.try_spawn(move || {
            sleep(Duration::from_secs(1));
        });

        // Then
        assert_eq!(second_spawn, Ok(()));
    }

    #[tokio::test]
    async fn can_spawn_10_tasks_when_limit_is_10() {
        // Given
        let number_of_pending_tasks = 10;
        let heavy_task_processor =
            SyncProcessor::new("Test", 1, number_of_pending_tasks).unwrap();

        for _ in 0..number_of_pending_tasks {
            // When
            let result = heavy_task_processor.try_spawn(move || {
                sleep(Duration::from_secs(1));
            });

            // Then
            assert_eq!(result, Ok(()));
        }
    }

    #[tokio::test]
    async fn executes_10_tasks_for_10_seconds_with_one_thread() {
        // Given
        let number_of_pending_tasks = 10;
        let number_of_threads = 1;
        let heavy_task_processor =
            SyncProcessor::new("Test", number_of_threads, number_of_pending_tasks)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..number_of_pending_tasks {
            let broadcast_sender = broadcast_sender.clone();
            let result = heavy_task_processor.try_spawn(move || {
                sleep(Duration::from_secs(1));
                broadcast_sender.send(()).unwrap();
            });
            assert_eq!(result, Ok(()));
        }
        drop(broadcast_sender);

        // Then
        while broadcast_receiver.recv().await.is_ok() {}
        assert!(instant.elapsed() >= Duration::from_secs(10));
        let duration = Duration::from_nanos(heavy_task_processor.metric.busy.get());
        assert_eq!(duration.as_secs(), 10);
    }

    #[tokio::test]
    async fn executes_10_tasks_for_10_seconds_with_one_thread_check_ordering() {
        // Given
        let number_of_pending_tasks = 10;
        let number_of_threads = 1;
        let heavy_task_processor =
            SyncProcessor::new("Test", number_of_threads, number_of_pending_tasks)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for i in 0..number_of_pending_tasks {
            let broadcast_sender = broadcast_sender.clone();
            let result = heavy_task_processor.try_spawn(move || {
                sleep(Duration::from_secs(1));
                broadcast_sender.send(i).unwrap();
            });
            assert_eq!(result, Ok(()));
        }
        drop(broadcast_sender);

        // Then
        let mut i = 0;
        loop {
            let Ok(result) = broadcast_receiver.recv().await else {
                break;
            };
            assert_eq!(result, i);
            i += 1;
        }
        assert_eq!(i, number_of_pending_tasks);
        assert!(instant.elapsed() >= Duration::from_secs(10));
    }

    #[tokio::test]
    async fn executes_10_tasks_for_2_seconds_with_10_thread() {
        // Given
        let number_of_pending_tasks = 10;
        let number_of_threads = 10;
        let heavy_task_processor =
            SyncProcessor::new("Test", number_of_threads, number_of_pending_tasks)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..number_of_pending_tasks {
            let broadcast_sender = broadcast_sender.clone();
            let result = heavy_task_processor.try_spawn(move || {
                sleep(Duration::from_secs(1));
                broadcast_sender.send(()).unwrap();
            });
            assert_eq!(result, Ok(()));
        }
        drop(broadcast_sender);

        // Then
        while broadcast_receiver.recv().await.is_ok() {}
        assert!(instant.elapsed() <= Duration::from_secs(2));
        let duration = Duration::from_nanos(heavy_task_processor.metric.busy.get());
        assert_eq!(duration.as_secs(), 10);
    }
}
