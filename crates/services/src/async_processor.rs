use fuel_core_metrics::futures::{
    metered_future::MeteredFuture,
    FuturesMetrics,
};
use std::{
    future::Future,
    sync::Arc,
};
use tokio::{
    runtime,
    sync::{
        OwnedSemaphorePermit,
        Semaphore,
    },
    task::JoinHandle,
};

/// A processor that can execute async tasks with a limit on the number of tasks that can be
/// executed concurrently.
pub struct AsyncProcessor {
    metric: FuturesMetrics,
    semaphore: Arc<Semaphore>,
    thread_pool: Option<runtime::Runtime>,
}

impl Drop for AsyncProcessor {
    fn drop(&mut self) {
        if let Some(runtime) = self.thread_pool.take() {
            runtime.shutdown_background();
        }
    }
}

/// A reservation for a task to be executed by the `AsyncProcessor`.
pub struct AsyncReservation(OwnedSemaphorePermit);

/// Out of capacity error.
#[derive(Debug, PartialEq, Eq)]
pub struct OutOfCapacity;

impl AsyncProcessor {
    /// Create a new `AsyncProcessor` with the given number of threads and the number of pending
    /// tasks.
    pub fn new(
        metric_name: &str,
        number_of_threads: usize,
        number_of_pending_tasks: usize,
    ) -> anyhow::Result<Self> {
        let thread_pool = if number_of_threads != 0 {
            let runtime = runtime::Builder::new_multi_thread()
                .worker_threads(number_of_threads)
                .enable_all()
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to create a tokio pool: {}", e))?;

            Some(runtime)
        } else {
            None
        };
        let semaphore = Arc::new(Semaphore::new(number_of_pending_tasks));
        let metric = FuturesMetrics::obtain_futures_metrics(metric_name);
        Ok(Self {
            metric,
            thread_pool,
            semaphore,
        })
    }

    /// Reserve a slot for a task to be executed.
    pub fn reserve(&self) -> Result<AsyncReservation, OutOfCapacity> {
        let permit = self.semaphore.clone().try_acquire_owned();
        if let Ok(permit) = permit {
            Ok(AsyncReservation(permit))
        } else {
            Err(OutOfCapacity)
        }
    }

    /// Spawn a task with a reservation.
    pub fn spawn_reserved<F>(
        &self,
        reservation: AsyncReservation,
        future: F,
    ) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        let permit = reservation.0;
        let future = async move {
            let permit = permit;
            let result = future.await;
            drop(permit);
            result
        };
        let metered_future = MeteredFuture::new(future, self.metric.clone());
        if let Some(runtime) = &self.thread_pool {
            runtime.spawn(metered_future)
        } else {
            tokio::spawn(metered_future)
        }
    }

    /// Tries to spawn a task. If the task cannot be spawned, returns an error.
    pub fn try_spawn<F>(&self, future: F) -> Result<JoinHandle<F::Output>, OutOfCapacity>
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        let reservation = self.reserve()?;
        Ok(self.spawn_reserved(reservation, future))
    }
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use futures::future::join_all;
    use std::{
        collections::HashSet,
        iter,
        thread::sleep,
        time::Duration,
    };
    use tokio::time::Instant;

    #[tokio::test]
    async fn one_spawn_single_tasks_works() {
        // Given
        const NUMBER_OF_PENDING_TASKS: usize = 1;
        let heavy_task_processor =
            AsyncProcessor::new("Test", 1, NUMBER_OF_PENDING_TASKS).unwrap();

        // When
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let result = heavy_task_processor.try_spawn(async move {
            sender.send(()).unwrap();
        });

        // Then
        result.expect("Expected Ok result");
        tokio::time::timeout(Duration::from_secs(5), receiver)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn one_spawn_single_tasks_works__thread_id_is_different_than_main() {
        // Given
        const MAX_NUMBER_OF_THREADS: usize = 10;
        const NUMBER_OF_PENDING_TASKS: usize = 10000;
        let heavy_task_processor =
            AsyncProcessor::new("Test", MAX_NUMBER_OF_THREADS, NUMBER_OF_PENDING_TASKS)
                .unwrap();
        let main_handler = tokio::spawn(async move { std::thread::current().id() });
        let main_id = main_handler.await.unwrap();

        // When
        let futures = iter::repeat_with(|| {
            heavy_task_processor
                .try_spawn(async move { std::thread::current().id() })
                .unwrap()
        })
        .take(NUMBER_OF_PENDING_TASKS)
        .collect::<Vec<_>>();

        // Then
        let thread_ids = join_all(futures).await;
        let unique_thread_ids = thread_ids
            .into_iter()
            .map(|r| r.unwrap())
            .collect::<HashSet<_>>();

        // Main thread was not used.
        assert!(!unique_thread_ids.contains(&main_id));
        // There's been at least one worker thread used.
        assert!(!unique_thread_ids.is_empty());
        // There were no more worker threads above the threshold.
        assert!(unique_thread_ids.len() <= MAX_NUMBER_OF_THREADS);
    }

    #[test]
    fn second_spawn_fails_when_limit_is_one_and_first_in_progress() {
        // Given
        const NUMBER_OF_PENDING_TASKS: usize = 1;
        let heavy_task_processor =
            AsyncProcessor::new("Test", 1, NUMBER_OF_PENDING_TASKS).unwrap();
        let first_spawn_result = heavy_task_processor.try_spawn(async move {
            sleep(Duration::from_secs(1));
        });
        first_spawn_result.expect("Expected Ok result");

        // When
        let second_spawn_result = heavy_task_processor.try_spawn(async move {
            sleep(Duration::from_secs(1));
        });

        // Then
        let err = second_spawn_result.expect_err("Should error");
        assert_eq!(err, OutOfCapacity);
    }

    #[tokio::test]
    async fn second_spawn_works_when_first_is_finished() {
        const NUMBER_OF_PENDING_TASKS: usize = 1;
        let heavy_task_processor =
            AsyncProcessor::new("Test", 1, NUMBER_OF_PENDING_TASKS).unwrap();

        // Given
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let first_spawn = heavy_task_processor.try_spawn(async move {
            sleep(Duration::from_secs(1));
            sender.send(()).unwrap();
        });
        first_spawn.expect("Expected Ok result").await.unwrap();
        receiver.await.unwrap();

        // When
        let second_spawn = heavy_task_processor.try_spawn(async move {
            sleep(Duration::from_secs(1));
        });

        // Then
        second_spawn.expect("Expected Ok result");
    }

    #[test]
    fn can_spawn_10_tasks_when_limit_is_10() {
        // Given
        const NUMBER_OF_PENDING_TASKS: usize = 10;
        let heavy_task_processor =
            AsyncProcessor::new("Test", 1, NUMBER_OF_PENDING_TASKS).unwrap();

        for _ in 0..NUMBER_OF_PENDING_TASKS {
            // When
            let result = heavy_task_processor.try_spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
            });

            // Then
            result.expect("Expected Ok result");
        }
    }

    #[tokio::test]
    async fn executes_5_tasks_for_5_seconds_with_one_thread() {
        // Given
        const NUMBER_OF_PENDING_TASKS: usize = 5;
        const NUMBER_OF_THREADS: usize = 1;
        let heavy_task_processor =
            AsyncProcessor::new("Test", NUMBER_OF_THREADS, NUMBER_OF_PENDING_TASKS)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..NUMBER_OF_PENDING_TASKS {
            let broadcast_sender = broadcast_sender.clone();
            let result = heavy_task_processor.try_spawn(async move {
                sleep(Duration::from_secs(1));
                broadcast_sender.send(()).unwrap();
            });
            result.expect("Expected Ok result");
        }
        drop(broadcast_sender);

        // Then
        while broadcast_receiver.recv().await.is_ok() {}
        // 5 tasks running on 1 thread, each task taking 1 second,
        // should complete in approximately 5 seconds overall.
        // Allowing some LEEWAY to account for runtime overhead.
        const LEEWAY: Duration = Duration::from_millis(300);
        assert!(instant.elapsed() < Duration::from_secs(5) + LEEWAY);
        // Make sure that the tasks were not executed in parallel.
        assert!(instant.elapsed() >= Duration::from_secs(5));
        // Wait for the metrics to be updated.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let duration = Duration::from_nanos(heavy_task_processor.metric.busy.get());
        assert_eq!(duration.as_secs(), 5);
        let duration = Duration::from_nanos(heavy_task_processor.metric.idle.get());
        assert_eq!(duration.as_secs(), 0);
    }

    #[tokio::test]
    async fn executes_10_blocking_tasks_for_1_second_with_10_threads__records_busy_time()
    {
        // Given
        const NUMBER_OF_PENDING_TASKS: usize = 10;
        const NUMBER_OF_THREADS: usize = 10;
        let heavy_task_processor =
            AsyncProcessor::new("Test", NUMBER_OF_THREADS, NUMBER_OF_PENDING_TASKS)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..NUMBER_OF_PENDING_TASKS {
            let broadcast_sender = broadcast_sender.clone();
            let result = heavy_task_processor.try_spawn(async move {
                sleep(Duration::from_secs(1));
                broadcast_sender.send(()).unwrap();
            });
            result.expect("Expected Ok result");
        }
        drop(broadcast_sender);

        // Then
        while broadcast_receiver.recv().await.is_ok() {}
        // 10 blocking tasks running on 10 threads, each task taking 1 second,
        // should complete in approximately 1 second overall.
        // Allowing some LEEWAY to account for runtime overhead.
        const LEEWAY: Duration = Duration::from_millis(300);
        assert!(instant.elapsed() <= Duration::from_secs(1) + LEEWAY);
        // Wait for the metrics to be updated.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let duration = Duration::from_nanos(heavy_task_processor.metric.busy.get());
        assert_eq!(duration.as_secs(), 10);
        let duration = Duration::from_nanos(heavy_task_processor.metric.idle.get());
        assert_eq!(duration.as_secs(), 0);
    }

    #[tokio::test]
    async fn executes_10_non_blocking_tasks_for_1_second_with_10_threads__records_idle_time(
    ) {
        // Given
        const NUMBER_OF_PENDING_TASKS: usize = 10;
        const NUMBER_OF_THREADS: usize = 10;
        let heavy_task_processor =
            AsyncProcessor::new("Test", NUMBER_OF_THREADS, NUMBER_OF_PENDING_TASKS)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..NUMBER_OF_PENDING_TASKS {
            let broadcast_sender = broadcast_sender.clone();
            let result = heavy_task_processor.try_spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                broadcast_sender.send(()).unwrap();
            });
            result.expect("Expected Ok result");
        }
        drop(broadcast_sender);

        // Then
        while broadcast_receiver.recv().await.is_ok() {}
        // 10 non-blocking tasks running on 10 threads, each task taking 1 second,
        // should complete in approximately 1 second overall.
        // Allowing some LEEWAY to account for runtime overhead.
        const LEEWAY: Duration = Duration::from_millis(300);
        assert!(instant.elapsed() <= Duration::from_secs(1) + LEEWAY);
        // Wait for the metrics to be updated.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let duration = Duration::from_nanos(heavy_task_processor.metric.busy.get());
        assert_eq!(duration.as_secs(), 0);
        let duration = Duration::from_nanos(heavy_task_processor.metric.idle.get());
        assert_eq!(duration.as_secs(), 10);
    }
}
