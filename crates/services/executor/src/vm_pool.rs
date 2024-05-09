use core::fmt;
use std::sync::{
    Arc,
    Mutex,
};

use fuel_core_types::fuel_vm::interpreter::Memory;

#[derive(Default, Clone)]
pub(crate) struct VmPool {
    pool: Arc<Mutex<Vec<Memory>>>,
}
impl VmPool {
    /// Gets a new VM memory instance from the pool.
    pub(crate) fn get_new(&self) -> Memory {
        let mut pool = self.pool.lock().expect("poisoned");
        pool.pop().unwrap_or_else(Memory::new)
    }

    /// Recycles a VM memory instance back into the pool.
    pub(crate) fn recycle(&self, mut mem: Memory) {
        mem.reset();
        let mut pool = self.pool.lock().expect("poisoned");
        pool.push(mem);
    }
}

impl fmt::Debug for VmPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pool.lock() {
            Ok(pool) => write!(f, "VmPool {{ pool: [{} items] }}", pool.len()),
            Err(_) => write!(f, "VmPool {{ pool: [poisoned] }}"),
        }
    }
}

#[test]
fn test_vm_pool() {
    let pool = VmPool::default();

    let mut mem = pool.get_new();
    mem.grow_stack(1024).expect("Unable to grow stack");
    mem.write_bytes_noownerchecks(0, [1, 2, 3, 4])
        .expect("Unable to write stack");
    let ptr1 = mem.stack_raw() as *const _ as *const u8 as usize;
    pool.recycle(mem);

    // Make sure we get the same memory allocation back
    let mem = pool.get_new();
    let ptr2 = mem.stack_raw() as *const _ as *const u8 as usize;
    assert_eq!(ptr1, ptr2);
}
