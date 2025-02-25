pub struct LocalRunner {
    process_thread: rayon::ThreadPool,
}

impl LocalRunner {
    pub fn new() -> anyhow::Result<Self> {
        let process_thread = rayon::ThreadPoolBuilder::new()
            // 1 thread for execution, 1 thread for block serialization
            .num_threads(2)
            .build()
            .map_err(|e|{
                anyhow::anyhow!("Failed to create a thread pool for the block processing: {e}")
            })?;

        Ok(Self { process_thread })
    }

    pub fn run<OP, Output>(&self, op: OP) -> Output
    where
        OP: FnOnce() -> Output,
        OP: Send,
        Output: Send,
    {
        self.process_thread.install(op)
    }

    pub fn run_in_parallel<A, RA, B, RB>(&self, oper_a: A, oper_b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        self.process_thread.join(oper_a, oper_b)
    }
}

#[test]
fn local_executor_executes_two_tasks_in_parallel() {
    use std::time::{
        Duration,
        Instant,
    };

    let runner = LocalRunner::new().unwrap();

    let sleep_time = Duration::from_secs(5);

    let start = Instant::now();

    // Given
    let task_1 = || std::thread::sleep(sleep_time);
    let task_2 = || std::thread::sleep(sleep_time);

    // When
    runner.run_in_parallel(task_1, task_2);

    // Then
    let elapsed = start.elapsed();
    assert!(elapsed < 2 * sleep_time);
}
