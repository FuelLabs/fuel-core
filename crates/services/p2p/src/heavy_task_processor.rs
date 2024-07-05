use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct HeavyTaskProcessor {
    rayon_thread_pool: rayon::ThreadPool,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OutOfCapacity;

impl HeavyTaskProcessor {
    pub fn new(
        number_of_threads: usize,
        number_of_pending_tasks: usize,
    ) -> anyhow::Result<Self> {
        let rayon_thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(number_of_threads)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create a rayon pool: {}", e))?;
        let semaphore = Arc::new(Semaphore::new(number_of_pending_tasks));
        Ok(Self {
            rayon_thread_pool,
            semaphore,
        })
    }

    pub fn spawn<OP>(&self, op: OP) -> Result<(), OutOfCapacity>
    where
        OP: FnOnce() + Send + 'static,
    {
        let permit = self.semaphore.clone().try_acquire_owned();

        if let Ok(permit) = permit {
            self.rayon_thread_pool.spawn_fifo(move || {
                let _drop = permit;
                op();
            });
            Ok(())
        } else {
            Err(OutOfCapacity)
        }
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
            HeavyTaskProcessor::new(1, number_of_pending_tasks).unwrap();

        // When
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let result = heavy_task_processor.spawn(|| {
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
            HeavyTaskProcessor::new(1, number_of_pending_tasks).unwrap();
        let first_spawn_result = heavy_task_processor.spawn(|| {
            sleep(Duration::from_secs(1));
        });
        assert_eq!(first_spawn_result, Ok(()));

        // When
        let second_spawn_result = heavy_task_processor.spawn(|| {
            sleep(Duration::from_secs(1));
        });

        // Then
        assert_eq!(second_spawn_result, Err(OutOfCapacity));
    }

    #[tokio::test]
    async fn second_spawn_works_when_first_is_finished() {
        let number_of_pending_tasks = 1;
        let heavy_task_processor =
            HeavyTaskProcessor::new(1, number_of_pending_tasks).unwrap();

        // Given
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let first_spawn = heavy_task_processor.spawn(|| {
            sleep(Duration::from_secs(1));
            sender.send(()).unwrap();
        });
        assert_eq!(first_spawn, Ok(()));
        receiver.await.unwrap();

        // When
        let second_spawn = heavy_task_processor.spawn(|| {
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
            HeavyTaskProcessor::new(1, number_of_pending_tasks).unwrap();

        for _ in 0..number_of_pending_tasks {
            // When
            let result = heavy_task_processor.spawn(|| {
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
            HeavyTaskProcessor::new(number_of_threads, number_of_pending_tasks).unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..number_of_pending_tasks {
            let broadcast_sender = broadcast_sender.clone();
            let result = heavy_task_processor.spawn(move || {
                sleep(Duration::from_secs(1));
                broadcast_sender.send(()).unwrap();
            });
            assert_eq!(result, Ok(()));
        }
        drop(broadcast_sender);

        // Then
        while broadcast_receiver.recv().await.is_ok() {}
        assert!(instant.elapsed() >= Duration::from_secs(10));
    }

    #[tokio::test]
    async fn executes_10_tasks_for_2_seconds_with_10_thread() {
        // Given
        let number_of_pending_tasks = 10;
        let number_of_threads = 10;
        let heavy_task_processor =
            HeavyTaskProcessor::new(number_of_threads, number_of_pending_tasks).unwrap();

        // When
        let (broadcast_sender, mut broadcast_receiver) =
            tokio::sync::broadcast::channel(1024);
        let instant = Instant::now();
        for _ in 0..number_of_pending_tasks {
            let broadcast_sender = broadcast_sender.clone();
            let result = heavy_task_processor.spawn(move || {
                sleep(Duration::from_secs(1));
                broadcast_sender.send(()).unwrap();
            });
            assert_eq!(result, Ok(()));
        }
        drop(broadcast_sender);

        // Then
        while broadcast_receiver.recv().await.is_ok() {}
        assert!(instant.elapsed() <= Duration::from_secs(2));
    }
}
