use core::{fmt, mem};
use fuel_core_types::fuel_vm::interpreter::MemoryInstance;
use std::sync::{Arc, Mutex};
use tokio::sync::OwnedSemaphorePermit;

/// Memory instance originating from a pool.
/// Will be recycled back into the pool when dropped.
pub struct MemoryFromPool {
    pool: MemoryPool,
    memory: MemoryInstance,
    _permit: OwnedSemaphorePermit,
}

impl Drop for MemoryFromPool {
    fn drop(&mut self) {
        self.pool.recycle_raw(mem::take(&mut self.memory));
    }
}

impl fmt::Debug for MemoryFromPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryFromPool")
            .field("pool", &"..")
            .field("memory", &self.memory)
            .finish()
    }
}

impl AsRef<MemoryInstance> for MemoryFromPool {
    fn as_ref(&self) -> &MemoryInstance {
        self.memory.as_ref()
    }
}

impl AsMut<MemoryInstance> for MemoryFromPool {
    fn as_mut(&mut self) -> &mut MemoryInstance {
        self.memory.as_mut()
    }
}

/// Pool of VM memory instances for reuse.
#[derive(Clone)]
pub struct MemoryPool {
    semaphore: Arc<tokio::sync::Semaphore>,
    pool: Arc<Mutex<Vec<MemoryInstance>>>,
}
impl MemoryPool {
    pub fn new(number_of_instances: usize) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(number_of_instances)),
            pool: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Gets a new raw VM memory instance from the pool.
    pub async fn take_raw(&self) -> MemoryFromPool {
        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("Semaphore is not closed");
        let mut pool = self.pool.lock().expect("poisoned");
        let memory = pool.pop().unwrap_or_default();

        MemoryFromPool {
            pool: self.clone(),
            memory,
            _permit,
        }
    }

    /// Adds a new memory instance to the pool.
    fn recycle_raw(&self, mut mem: MemoryInstance) {
        mem.reset();
        let mut pool = self.pool.lock().expect("poisoned");
        pool.push(mem);
    }
}

impl fmt::Debug for MemoryPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pool.lock() {
            Ok(pool) => {
                write!(f, "SharedVmMemoryPool {{ pool: [{} items] }}", pool.len())
            }
            Err(_) => write!(f, "SharedVmMemoryPool {{ pool: [poisoned] }}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn memory_pool_recycling_works() {
        // Given
        let pool = MemoryPool::new(1);

        // When
        let mut mem_guard = pool.take_raw().await;
        let mem = mem_guard.as_mut();
        mem.grow_stack(1024).expect("Unable to grow stack");
        mem.write_bytes_noownerchecks(0, [1, 2, 3, 4])
            .expect("Unable to write stack");
        let ptr1 = mem.stack_raw() as *const _ as *const u8 as usize;
        drop(mem_guard);

        // Then
        // Make sure we get the same memory allocation back
        let mem = pool.take_raw().await;
        let ptr2 = mem.as_ref().stack_raw() as *const _ as *const u8 as usize;
        assert_eq!(ptr1, ptr2);
    }

    #[tokio::test]
    async fn memory_pool_locking_works() {
        // Given
        const POOL_SIZE: usize = 4;
        let pool = MemoryPool::new(POOL_SIZE);
        let mut _drop = vec![];
        for _ in 0..POOL_SIZE {
            _drop.push(pool.take_raw().await);
        }

        // When
        let mem = tokio::time::timeout(Duration::from_secs(1), pool.take_raw()).await;

        // Then
        assert!(mem.is_err());
    }

    #[tokio::test]
    async fn memory_pool_freeing_works() {
        // Given
        const POOL_SIZE: usize = 4;
        let pool = MemoryPool::new(POOL_SIZE);
        let mut _drop = vec![];
        for _ in 0..POOL_SIZE {
            _drop.push(pool.take_raw().await);
        }
        drop(_drop);

        // When
        let mem = tokio::time::timeout(Duration::from_secs(1), pool.take_raw()).await;

        // Then
        assert!(mem.is_ok());
    }
}
