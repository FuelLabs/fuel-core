//! A pool of recyclable VM memory instances.

use alloc::{
    sync::Arc,
    vec::Vec,
};
use core::{
    fmt,
    mem,
};
use fuel_vm_private::interpreter::MemoryInstance;
use parking_lot::Mutex;

/// The recyclable memory instance originating from a pool.
pub struct RecyclableMemory {
    pool: MemoryPool,
    memory: MemoryInstance,
}

impl Drop for RecyclableMemory {
    fn drop(&mut self) {
        self.pool.recycle_raw(mem::take(&mut self.memory));
    }
}

impl fmt::Debug for RecyclableMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryFromPool")
            .field("memory", &self.memory)
            .finish()
    }
}

impl AsRef<MemoryInstance> for RecyclableMemory {
    fn as_ref(&self) -> &MemoryInstance {
        self.memory.as_ref()
    }
}

impl AsMut<MemoryInstance> for RecyclableMemory {
    fn as_mut(&mut self) -> &mut MemoryInstance {
        self.memory.as_mut()
    }
}

#[derive(Clone)]
/// THe pool of VM memory instances for reuse.
pub struct MemoryPool {
    pool: Arc<Mutex<Vec<MemoryInstance>>>,
}

impl Default for MemoryPool {
    fn default() -> Self {
        Self {
            pool: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl MemoryPool {
    /// Creates a new memory pool.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets a new raw VM memory instance from the pool.
    pub fn take_raw(&self) -> RecyclableMemory {
        let mut pool = self.pool.lock();
        let memory = pool.pop().unwrap_or_default();

        RecyclableMemory {
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
