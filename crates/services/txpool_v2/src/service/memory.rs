use core::fmt;
use fuel_core_types::fuel_vm::interpreter::MemoryInstance;
use parking_lot::Mutex;
use std::{
    mem,
    sync::Arc,
};

pub struct MemoryFromPool {
    pool: MemoryPool,
    memory: MemoryInstance,
}

impl Drop for MemoryFromPool {
    fn drop(&mut self) {
        self.pool.recycle_raw(mem::take(&mut self.memory));
    }
}

impl fmt::Debug for MemoryFromPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryFromPool")
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

#[derive(Clone)]
pub struct MemoryPool {
    pool: Arc<Mutex<Vec<MemoryInstance>>>,
}

impl MemoryPool {
    pub fn new() -> Self {
        Self {
            pool: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Gets a new raw VM memory instance from the pool.
    pub fn take_raw(&self) -> MemoryFromPool {
        let memory = {
            let mut pool = self.pool.lock();
            pool.pop().unwrap_or_default()
        };

        MemoryFromPool {
            pool: self.clone(),
            memory,
        }
    }

    /// Adds a new memory instance to the pool.
    fn recycle_raw(&self, mut mem: MemoryInstance) {
        mem.reset();
        let mut pool = self.pool.lock();
        pool.push(mem);
    }
}
