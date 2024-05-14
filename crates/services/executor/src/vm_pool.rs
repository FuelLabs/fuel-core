use core::{
    fmt,
    mem,
};
use std::sync::{
    Arc,
    Mutex,
};

use fuel_core_types::fuel_vm::{
    interpreter::Memory,
    pool::VmMemoryPool,
};

/// Memory instance originating from a pool.
/// Will be recycled back into the pool when dropped.
pub struct MemoryFromPool {
    pool: SharedVmMemoryPool,
    memory: Memory,
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

impl AsRef<Memory> for MemoryFromPool {
    fn as_ref(&self) -> &Memory {
        self.memory.as_ref()
    }
}

impl AsMut<Memory> for MemoryFromPool {
    fn as_mut(&mut self) -> &mut Memory {
        self.memory.as_mut()
    }
}

/// Pool of VM memory instances for reuse.
#[derive(Default, Clone)]
pub struct SharedVmMemoryPool {
    pool: Arc<Mutex<Vec<Memory>>>,
}
impl SharedVmMemoryPool {
    /// Gets a new raw VM memory instance from the pool.
    fn take_raw(&self) -> Memory {
        let mut pool = self.pool.lock().expect("poisoned");
        pool.pop().unwrap_or_default()
    }

    /// Adds a new memory instance to the pool.
    fn recycle_raw(&self, mut mem: Memory) {
        mem.reset();
        let mut pool = self.pool.lock().expect("poisoned");
        pool.push(mem);
    }
}

impl VmMemoryPool for SharedVmMemoryPool {
    type Memory = MemoryFromPool;

    fn get_new(&self) -> Self::Memory {
        MemoryFromPool {
            pool: self.clone(),
            memory: self.take_raw(),
        }
    }

    fn recycle(&self, _mem: MemoryFromPool) {
        // Just drop it, it will recycle automatically on drop
    }
}

impl fmt::Debug for SharedVmMemoryPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pool.lock() {
            Ok(pool) => {
                write!(f, "SharedVmMemoryPool {{ pool: [{} items] }}", pool.len())
            }
            Err(_) => write!(f, "SharedVmMemoryPool {{ pool: [poisoned] }}"),
        }
    }
}

#[test]
fn test_vm_memory_pool() {
    let pool = SharedVmMemoryPool::default();

    let mut mem_guard = pool.get_new();
    let mem = mem_guard.as_mut();
    mem.grow_stack(1024).expect("Unable to grow stack");
    mem.write_bytes_noownerchecks(0, [1, 2, 3, 4])
        .expect("Unable to write stack");
    let ptr1 = mem.stack_raw() as *const _ as *const u8 as usize;
    drop(mem_guard);

    // Make sure we get the same memory allocation back
    let mem = pool.get_new();
    let ptr2 = mem.as_ref().stack_raw() as *const _ as *const u8 as usize;
    assert_eq!(ptr1, ptr2);
}
