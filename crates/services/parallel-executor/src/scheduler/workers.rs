use std::sync::Arc;

use parking_lot::Mutex;

#[derive(Clone)]
pub struct WorkerPool {
    workers: Arc<Mutex<usize>>,
}

pub struct WorkerId {
    pool: WorkerPool,
    pub _id: usize,
}

impl Drop for WorkerId {
    fn drop(&mut self) {
        self.pool.return_worker();
    }
}

impl WorkerPool {
    pub fn new(size: usize) -> Self {
        Self {
            workers: Arc::new(Mutex::new(size)),
        }
    }

    pub fn take_worker(&self) -> Option<WorkerId> {
        let mut workers = self.workers.lock();
        if *workers > 0 {
            *workers -= 1;
            Some(WorkerId {
                pool: self.clone(),
                _id: *workers,
            })
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        *self.workers.lock() == 0
    }

    pub fn return_worker(&self) {
        let mut workers = self.workers.lock();
        *workers = (*workers).saturating_add(1);
    }
}
