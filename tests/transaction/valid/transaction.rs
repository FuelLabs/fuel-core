use super::d;
use fuel_vm_rust::consts::*;
use fuel_vm_rust::prelude::*;

#[test]
fn gas_price() {
    Transaction::script(MAX_GAS_PER_TX, d(), d(), d(), d(), d(), d(), d())
        .validate(1000)
        .unwrap();

    Transaction::create(
        MAX_GAS_PER_TX,
        d(),
        d(),
        0,
        d(),
        d(),
        d(),
        d(),
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .unwrap();

    let err = Transaction::script(MAX_GAS_PER_TX + 1, d(), d(), d(), d(), d(), d(), d())
        .validate(1000)
        .err()
        .unwrap();
    assert_eq!(ValidationError::TransactionGasLimit, err);

    let err = Transaction::create(
        MAX_GAS_PER_TX + 1,
        d(),
        d(),
        0,
        d(),
        d(),
        d(),
        d(),
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionGasLimit, err);
}

#[test]
fn maturity() {
    Transaction::script(d(), d(), 1000, d(), d(), d(), d(), d())
        .validate(1000)
        .unwrap();

    Transaction::create(d(), d(), 1000, 0, d(), d(), d(), d(), vec![vec![0xfau8].into()])
        .validate(1000)
        .unwrap();

    let err = Transaction::script(d(), d(), 1001, d(), d(), d(), d(), d())
        .validate(1000)
        .err()
        .unwrap();
    assert_eq!(ValidationError::TransactionMaturity, err);

    let err = Transaction::create(d(), d(), 1001, 0, d(), d(), d(), d(), vec![vec![0xfau8].into()])
        .validate(1000)
        .err()
        .unwrap();
    assert_eq!(ValidationError::TransactionMaturity, err);
}

#[test]
fn max_iow() {
    Transaction::script(
        d(),
        d(),
        d(),
        d(),
        d(),
        vec![Input::coin(d(), d(), d(), d(), d(), d(), d(), d()); MAX_INPUTS],
        vec![Output::coin(d(), d(), d()); MAX_OUTPUTS],
        vec![vec![0xfau8].into(); MAX_WITNESSES],
    )
    .validate(1000)
    .unwrap();

    Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![Input::coin(d(), d(), d(), d(), d(), d(), d(), d()); MAX_INPUTS],
        vec![Output::coin(d(), d(), d()); MAX_OUTPUTS],
        vec![vec![0xfau8].into(); MAX_WITNESSES],
    )
    .validate(1000)
    .unwrap();

    let err = Transaction::script(
        d(),
        d(),
        d(),
        d(),
        d(),
        vec![Input::contract(d(), d(), d(), d()); MAX_INPUTS + 1],
        vec![Output::variable(d(), d(), d()); MAX_OUTPUTS],
        vec![vec![0xfau8].into(); MAX_WITNESSES],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionInputsMax, err);

    let err = Transaction::script(
        d(),
        d(),
        d(),
        d(),
        d(),
        vec![Input::contract(d(), d(), d(), d()); MAX_INPUTS],
        vec![Output::variable(d(), d(), d()); MAX_OUTPUTS + 1],
        vec![vec![0xfau8].into(); MAX_WITNESSES],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionOutputsMax, err);

    let err = Transaction::script(
        d(),
        d(),
        d(),
        d(),
        d(),
        vec![Input::contract(d(), d(), d(), d()); MAX_INPUTS],
        vec![Output::variable(d(), d(), d()); MAX_OUTPUTS],
        vec![vec![0xfau8].into(); MAX_WITNESSES + 1],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionWitnessesMax, err);
}

#[test]
fn output_change_color() {
    let mut a = Color::default();
    let mut b = Color::default();
    let mut c = Color::default();

    a[0] = 0xfa;
    b[0] = 0xfb;
    c[0] = 0xfc;

    Transaction::script(
        d(),
        d(),
        d(),
        d(),
        d(),
        vec![
            Input::coin(d(), d(), d(), a, 0, d(), d(), d()),
            Input::coin(d(), d(), d(), b, 0, d(), d(), d()),
        ],
        vec![Output::change(d(), d(), a), Output::change(d(), d(), b)],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .unwrap();

    let err = Transaction::script(
        d(),
        d(),
        d(),
        d(),
        d(),
        vec![
            Input::coin(d(), d(), d(), a, 0, d(), d(), d()),
            Input::coin(d(), d(), d(), b, 0, d(), d(), d()),
        ],
        vec![Output::change(d(), d(), a), Output::change(d(), d(), a)],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionOutputChangeColorDuplicated, err);

    let err = Transaction::script(
        d(),
        d(),
        d(),
        d(),
        d(),
        vec![
            Input::coin(d(), d(), d(), a, 0, d(), d(), d()),
            Input::coin(d(), d(), d(), b, 0, d(), d(), d()),
        ],
        vec![Output::change(d(), d(), a), Output::change(d(), d(), c)],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionOutputChangeColorNotFound, err);
}

#[test]
fn script() {
    Transaction::script(
        d(),
        d(),
        d(),
        vec![0xfa; MAX_SCRIPT_LENGTH],
        vec![0xfb; MAX_SCRIPT_DATA_LENGTH],
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .unwrap();

    let err = Transaction::script(
        d(),
        d(),
        d(),
        vec![0xfa; MAX_SCRIPT_LENGTH],
        vec![0xfb; MAX_SCRIPT_DATA_LENGTH],
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::contract_created(d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(
        ValidationError::TransactionScriptOutputContractCreated { index: 0 },
        err
    );

    let err = Transaction::script(
        d(),
        d(),
        d(),
        vec![0xfa; MAX_SCRIPT_LENGTH + 1],
        vec![0xfb; MAX_SCRIPT_DATA_LENGTH],
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionScriptLength, err);

    let err = Transaction::script(
        d(),
        d(),
        d(),
        vec![0xfa; MAX_SCRIPT_LENGTH],
        vec![0xfb; MAX_SCRIPT_DATA_LENGTH + 1],
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionScriptDataLength, err);
}

#[test]
fn create() {
    let mut a = Color::default();
    let mut b = Color::default();
    let mut c = Color::default();

    a[0] = 0xfa;
    b[0] = 0xfb;
    c[0] = 0xfc;

    Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .unwrap();

    let err = Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![Input::contract(d(), d(), d(), d())],
        vec![Output::contract(0, d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionCreateInputContract { index: 0 }, err);

    let err = Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::variable(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionCreateOutputVariable { index: 0 }, err);

    let err = Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![
            Input::coin(d(), d(), d(), d(), 0, d(), d(), d()),
            Input::coin(d(), d(), d(), a, 0, d(), d(), d()),
        ],
        vec![Output::change(d(), d(), d()), Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(
        ValidationError::TransactionCreateOutputChangeColorZero { index: 1 },
        err
    );

    let err = Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![
            Input::coin(d(), d(), d(), d(), 0, d(), d(), d()),
            Input::coin(d(), d(), d(), a, 0, d(), d(), d()),
        ],
        vec![Output::change(d(), d(), d()), Output::change(d(), d(), a)],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(
        ValidationError::TransactionCreateOutputChangeColorNonZero { index: 1 },
        err
    );

    let err = Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![
            Input::coin(d(), d(), d(), d(), 0, d(), d(), d()),
            Input::coin(d(), d(), d(), a, 0, d(), d(), d()),
        ],
        vec![Output::contract_created(d()); 2],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(
        ValidationError::TransactionCreateOutputContractCreatedMultiple { index: 1 },
        err
    );

    Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8; CONTRACT_MAX_SIZE as usize / 4].into()],
    )
    .validate(1000)
    .unwrap();

    let err = Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        d(),
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8; 1 + CONTRACT_MAX_SIZE as usize / 4].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionCreateBytecodeLen, err);

    let err = Transaction::create(
        d(),
        d(),
        d(),
        1,
        d(),
        d(),
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionCreateBytecodeWitnessIndex, err);

    let mut id = Id::default();
    let mut static_contracts = (0..MAX_STATIC_CONTRACTS as u64)
        .map(|i| {
            id[..8].copy_from_slice(&i.to_be_bytes());
            id
        })
        .collect::<Vec<Id>>();

    Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        static_contracts.clone(),
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .unwrap();

    id.iter_mut().for_each(|i| *i = 0xff);
    static_contracts.push(id);
    let err = Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        static_contracts.clone(),
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionCreateStaticContractsMax, err);

    static_contracts.pop();
    static_contracts[0][0] = 0xff;
    let err = Transaction::create(
        d(),
        d(),
        d(),
        0,
        d(),
        static_contracts,
        vec![Input::coin(d(), d(), d(), d(), 0, d(), d(), d())],
        vec![Output::change(d(), d(), d())],
        vec![vec![0xfau8].into()],
    )
    .validate(1000)
    .err()
    .unwrap();
    assert_eq!(ValidationError::TransactionCreateStaticContractsOrder, err);
}
