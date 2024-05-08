use std::cell::RefCell;

use fuel_core_types::fuel_vm::interpreter::Memory;

thread_local! {
    static POOL: RefCell<Vec<Memory>> = RefCell::new(Vec::new());
}

/// Gets a new VM memory instance from the pool.
pub(crate) fn get_vm_memory() -> Memory {
    POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        pool.pop().unwrap_or_else(Memory::new)
    })
}

/// Recycles a VM memory instance back into the pool.
pub(crate) fn recycle_vm_memory(mut mem: Memory) {
    POOL.with(|pool| {
        mem.reset();
        let mut pool = pool.borrow_mut();
        pool.push(mem);
    })
}

#[test]
fn test_vm_pool() {
    let mut mem = get_vm_memory();
    mem.grow_stack(1024).expect("Unable to grow stack");
    mem.write_bytes_noownerchecks(0, [1, 2, 3, 4])
        .expect("Unable to write stack");
    let ptr1 = mem.stack_raw() as *const _ as *const u8 as usize;
    recycle_vm_memory(mem);

    // Make sure we get the same memory allocation back
    let mem = get_vm_memory();
    let ptr2 = mem.stack_raw() as *const _ as *const u8 as usize;
    assert_eq!(ptr1, ptr2);
}
