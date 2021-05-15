use super::assert_encoding_correct;
use super::common::r;
use fuel_vm_rust::consts::*;
use fuel_vm_rust::prelude::*;

#[test]
fn call() {
    assert_encoding_correct(
        vec![
            (vec![], vec![]),
            (vec![(0..1024).into()], vec![]),
            (vec![], vec![(253..2048).into()]),
            (vec![(0..1024).into()], vec![(..2059).into()]),
            (vec![(0..1024).into(), (4096..5092).into()], vec![(..2059).into()]),
        ]
        .into_iter()
        .map(|(inputs, outputs)| Call::new(r(), inputs, outputs))
        .collect::<Vec<Call>>()
        .as_slice(),
    );
}

#[test]
fn call_frame() {
    assert_encoding_correct(
        vec![
            (vec![], vec![]),
            (vec![(0..1024).into()], vec![]),
            (vec![], vec![(253..2048).into()]),
            (vec![(0..1024).into()], vec![(..2059).into()]),
            (vec![(0..1024).into(), (4096..5092).into()], vec![(..2059).into()]),
        ]
        .into_iter()
        .map(|(inputs, outputs)| CallFrame::new(r(), r(), [r(); VM_REGISTER_COUNT], inputs, outputs, vec![r(); 200]))
        .collect::<Vec<CallFrame>>()
        .as_slice(),
    );
}
