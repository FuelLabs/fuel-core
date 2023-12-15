//! A simple job running framework for predefined jobs running repeatedly
//! at fixed intervals. This facility is not meant for work that needs
//! to be done on a tight deadline, solely for work that needs to be done
//! at reasonably long intervals (like a few hours)

use slog::{debug, info, o, trace, warn, Logger};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

/// An individual job to run. Each job should be written in a way that it
/// doesn't take more than a few minutes.
#[async_trait]
pub trait Job: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self, logger: &Logger);
}

struct Task {
    job: Arc<dyn Job>,
    logger: Logger,
    interval: Duration,
    next_run: Instant,
}

pub struct Runner {
    logger: Logger,
    tasks: Vec<Task>,
    pub stop: Arc<AtomicBool>,
}

impl Runner {
    pub fn new(logger: &Logger) -> Runner {
        Runner {
            logger: logger.new(o!("component" => "JobRunner")),
            tasks: Vec::new(),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn register(&mut self, job: Arc<dyn Job>, interval: Duration) {
        let logger = self.logger.new(o!("job" => job.name().to_owned()));
        // We want tasks to start running pretty soon after server start, but
        // also want to avoid that they all need to run at the same time. We
        // therefore run them a small fraction of their interval from now. For
        // a job that runs daily, we'd do the first run in about 15 minutes
        let next_run = Instant::now() + interval / 91;
        let task = Task {
            job,
            interval,
            logger,
            next_run,
        };
        self.tasks.push(task);
    }

    pub async fn start(mut self) {
        info!(
            self.logger,
            "Starting job runner with {} jobs",
            self.tasks.len()
        );

        for task in &self.tasks {
            let next = task.next_run.saturating_duration_since(Instant::now());
            debug!(self.logger, "Schedule for {}", task.job.name();
                   "interval_s" => task.interval.as_secs(),
                   "first_run_in_s" => next.as_secs());
        }

        while !self.stop.load(Ordering::SeqCst) {
            let now = Instant::now();
            let mut next = Instant::now() + Duration::from_secs(365 * 24 * 60 * 60);
            for task in self.tasks.iter_mut() {
                if task.next_run < now {
                    // This will become obnoxious pretty quickly
                    trace!(self.logger, "Running job"; "name" => task.job.name());
                    // We only run one job at a time since we don't want to
                    // deal with the same job possibly starting twice.
                    task.job.run(&task.logger).await;
                    task.next_run = Instant::now() + task.interval;
                }
                next = next.min(task.next_run);
            }
            let wait = next.saturating_duration_since(Instant::now());
            tokio::time::sleep(wait).await;
        }
        self.stop.store(false, Ordering::SeqCst);
        warn!(self.logger, "Received request to stop. Stopping runner");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;
    use std::sync::{Arc, Mutex};

    lazy_static! {
        pub static ref LOGGER: Logger = match crate::env::ENV_VARS.log_levels {
            Some(_) => crate::log::logger(false),
            None => Logger::root(slog::Discard, o!()),
        };
    }

    struct CounterJob {
        count: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl Job for CounterJob {
        fn name(&self) -> &str {
            "counter job"
        }

        async fn run(&self, _: &Logger) {
            let mut count = self.count.lock().expect("Failed to lock count");
            if *count < 10 {
                *count += 1;
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn jobs_run() {
        let count = Arc::new(Mutex::new(0));
        let job = CounterJob {
            count: count.clone(),
        };
        let mut runner = Runner::new(&LOGGER);
        runner.register(Arc::new(job), Duration::from_millis(10));
        let stop = runner.stop.clone();

        crate::spawn_blocking(runner.start());

        let start = Instant::now();
        loop {
            let current = { *count.lock().unwrap() };
            if current >= 10 {
                break;
            }
            if start.elapsed() > Duration::from_secs(2) {
                assert!(false, "Counting to 10 took longer than 2 seconds");
            }
        }

        stop.store(true, Ordering::SeqCst);
        // Wait for the runner to shut down
        while stop.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
