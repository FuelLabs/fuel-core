/*
use fuel_core::consts::*;
use fuel_core::prelude::*;

#[test]
fn call_output_ownership() {
    let mut vm = Interpreter::default();
    vm.init(Transaction::default()).unwrap();

    let bytes = 1024u64;

    // r[0x10] = bytes
    vm.execute(Opcode::AddI(0x10, 0x10, bytes as Immediate12)).unwrap();
    vm.execute(Opcode::Aloc(0x10)).unwrap();

    let (start, end) = MemorySlice::from(1024u64..).to_heap(&vm).boundaries().unwrap();
    assert!(vm.has_ownership_range(start, end));

    /*
    let call = Call::new(
        Default::default(),
        vec![],
        vec![MemorySlice::from(1024u64..).to_heap(&vm)],
    );
    assert!(call.has_outputs_ownership(&vm));
    */
}
*/
