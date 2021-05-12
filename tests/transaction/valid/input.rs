use super::d;
use fuel_vm_rust::consts::*;
use fuel_vm_rust::prelude::*;

#[test]
fn coin() {
    let witnesses = vec![vec![0xff; 128].into()];

    Input::coin(d(), d(), d(), d(), 0, d(), vec![0u8; MAX_PREDICATE_LENGTH], d())
        .validate(1, &[], witnesses.as_slice())
        .unwrap();

    Input::coin(d(), d(), d(), d(), 0, d(), d(), vec![0u8; MAX_PREDICATE_DATA_LENGTH])
        .validate(1, &[], witnesses.as_slice())
        .unwrap();

    let err = Input::coin(d(), d(), d(), d(), 0, d(), vec![0u8; MAX_PREDICATE_LENGTH + 1], d())
        .validate(1, &[], witnesses.as_slice())
        .err()
        .unwrap();
    assert_eq!(ValidationError::InputCoinPredicateLength { index: 1 }, err);

    let err = Input::coin(
        d(),
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![0u8; MAX_PREDICATE_DATA_LENGTH + 1],
    )
    .validate(1, &[], witnesses.as_slice())
    .err()
    .unwrap();
    assert_eq!(ValidationError::InputCoinPredicateDataLength { index: 1 }, err);

    let err = Input::coin(d(), d(), d(), d(), 1, d(), d(), d())
        .validate(1, &[], witnesses.as_slice())
        .err()
        .unwrap();
    assert_eq!(ValidationError::InputCoinWitnessIndexBounds { index: 1 }, err);
}

#[test]
fn contract() {
    Input::contract(d(), d(), d(), d())
        .validate(1, &[Output::contract(1, d(), d())], &[])
        .unwrap();

    let err = Input::contract(d(), d(), d(), d()).validate(1, &[], &[]).err().unwrap();
    assert_eq!(ValidationError::InputContractAssociatedOutputContract { index: 1 }, err);

    let err = Input::contract(d(), d(), d(), d())
        .validate(1, &[Output::coin(d(), d(), d())], &[])
        .err()
        .unwrap();
    assert_eq!(ValidationError::InputContractAssociatedOutputContract { index: 1 }, err);

    let err = Input::contract(d(), d(), d(), d())
        .validate(1, &[Output::contract(2, d(), d())], &[])
        .err()
        .unwrap();
    assert_eq!(ValidationError::InputContractAssociatedOutputContract { index: 1 }, err);
}
