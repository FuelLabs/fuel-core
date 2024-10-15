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

    #[test]
    fn one_spawn_single_tasks_works() {
        // Given
        let number_of_pending_tasks = 1;
        let heavy_task_processor =
            AsyncProcessor::new("Test", 1, number_of_pending_tasks).unwrap();

        // When
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let result = heavy_task_processor.try_spawn(async move {
            sender.send(()).unwrap();
        });

        // Then
        result.expect("Expected Ok result");
        sleep(Duration::from_secs(1));
        receiver.try_recv().unwrap();
    }

    #[tokio::test]
    async fn one_spawn_single_tasks_works__thread_id_is_different_than_main() {
        // Given
        let number_of_threads = 10;
        let number_of_pending_tasks = 10000;
        let heavy_task_processor =
            AsyncProcessor::new("Test", number_of_threads, number_of_pending_tasks)
                .unwrap();
        let main_handler = tokio::spawn(async move { std::thread::current().id() });
        let main_id = main_handler.await.unwrap();

        // When
        let futures = iter::repeat_with(|| {
            heavy_task_processor
                .try_spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    std::thread::current().id()
                })
                .unwrap()
        })
        .take(number_of_pending_tasks)
        .collect::<Vec<_>>();

        // Then
        let thread_ids = join_all(futures).await;
        let unique_thread_ids = thread_ids
            .into_iter()
            .map(|r| r.unwrap())
            .collect::<HashSet<_>>();

        assert!(!unique_thread_ids.contains(&main_id));
        assert_eq!(unique_thread_ids.len(), number_of_threads);
    }

    #[test]
    fn second_spawn_fails_when_limit_is_one_and_first_in_progress() {
        // Given
        let number_of_pending_tasks = 1;
        let heavy_task_processor =
            AsyncProcessor::new("Test", 1, number_of_pending_tasks).unwrap();
        let first_spawn_result = heavy_task_processor.try_spawn(async move {
            sleep(Duration::from_secs(1));
        });
        first_spawn_result.expect("Expected Ok result");

        // When
        let second_spawn_result = heavy_task_processor.try_spawn(async move {
            sleep(Duration::from_secs(1));
        });

        // Then
        let err = second_spawn_result.expect_err("Expected Ok result");
        assert_eq!(err, OutOfCapacity);
    }

    #[test]
    fn second_spawn_works_when_first_is_finished() {
        let number_of_pending_tasks = 1;
        let heavy_task_processor =
            AsyncProcessor::new("Test", 1, number_of_pending_tasks).unwrap();

        // Given
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let first_spawn = heavy_task_processor.try_spawn(async move {
            sleep(Duration::from_secs(1));
            sender.send(()).unwrap();
        });
        first_spawn.expect("Expected Ok result");
        futures::executor::block_on(async move {
            receiver.await.unwrap();
        });

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
        let number_of_pending_tasks = 10;
        let heavy_task_processor =
            AsyncProcessor::new("Test", 1, number_of_pending_tasks).unwrap();

        for _ in 0..number_of_pending_tasks {
            // When
            let result = heavy_task_processor.try_spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
            });

            // Then
            result.expect("Expected Ok result");
        }
    }

    #[tokio::test]
    async fn executes_10_tasks_for_10_seconds_with_one_thread() {
        // Given
        let number_of_pending_tasks = 10;
        let number_of_threads = 1;
        let heavy_task_processor =
            AsyncProcessor::new("Test", number_of_threads, number_of_pending_tasks)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..number_of_pending_tasks {
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
        assert!(instant.elapsed() >= Duration::from_secs(10));
        // Wait for the metrics to be updated.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let duration = Duration::from_nanos(heavy_task_processor.metric.busy.get());
        assert_eq!(duration.as_secs(), 10);
        let duration = Duration::from_nanos(heavy_task_processor.metric.idle.get());
        assert_eq!(duration.as_secs(), 0);
    }

    #[tokio::test]
    async fn executes_10_tasks_for_2_seconds_with_10_thread() {
        // Given
        let number_of_pending_tasks = 10;
        let number_of_threads = 10;
        let heavy_task_processor =
            AsyncProcessor::new("Test", number_of_threads, number_of_pending_tasks)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..number_of_pending_tasks {
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
        assert!(instant.elapsed() <= Duration::from_secs(2));
        // Wait for the metrics to be updated.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let duration = Duration::from_nanos(heavy_task_processor.metric.busy.get());
        assert_eq!(duration.as_secs(), 10);
        let duration = Duration::from_nanos(heavy_task_processor.metric.idle.get());
        assert_eq!(duration.as_secs(), 0);
    }

    #[tokio::test]
    async fn executes_10_tasks_for_2_seconds_with_1_thread() {
        // Given
        let number_of_pending_tasks = 10;
        let number_of_threads = 10;
        let heavy_task_processor =
            AsyncProcessor::new("Test", number_of_threads, number_of_pending_tasks)
                .unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..number_of_pending_tasks {
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
        assert!(instant.elapsed() <= Duration::from_secs(2));
        // Wait for the metrics to be updated.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let duration = Duration::from_nanos(heavy_task_processor.metric.busy.get());
        assert_eq!(duration.as_secs(), 0);
        let duration = Duration::from_nanos(heavy_task_processor.metric.idle.get());
        assert_eq!(duration.as_secs(), 10);
    }
}
