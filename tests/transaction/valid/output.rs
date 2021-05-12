use super::d;
use fuel_vm_rust::prelude::*;

#[test]
fn coin() {
    Output::coin(d(), d(), d()).validate(1, &[]).unwrap();
}

#[test]
fn contract() {
    Output::contract(1, d(), d())
        .validate(
            2,
            &[
                Input::coin(d(), d(), d(), d(), d(), d(), d(), d()),
                Input::contract(d(), d(), d(), d()),
            ],
        )
        .unwrap();

    let err = Output::contract(0, d(), d())
        .validate(
            2,
            &[
                Input::coin(d(), d(), d(), d(), d(), d(), d(), d()),
                Input::contract(d(), d(), d(), d()),
            ],
        )
        .err()
        .unwrap();
    assert_eq!(ValidationError::OutputContractInputIndex { index: 2 }, err);

    let err = Output::contract(2, d(), d())
        .validate(
            2,
            &[
                Input::coin(d(), d(), d(), d(), d(), d(), d(), d()),
                Input::contract(d(), d(), d(), d()),
            ],
        )
        .err()
        .unwrap();
    assert_eq!(ValidationError::OutputContractInputIndex { index: 2 }, err);
}

#[test]
fn withdrawal() {
    Output::withdrawal(d(), d(), d()).validate(1, &[]).unwrap();
}

#[test]
fn change() {
    Output::change(d(), d(), d()).validate(1, &[]).unwrap();
}

#[test]
fn variable() {
    Output::variable(d(), d(), d()).validate(1, &[]).unwrap();
}

#[test]
fn contract_created() {
    Output::contract_created(d()).validate(1, &[]).unwrap();
}
