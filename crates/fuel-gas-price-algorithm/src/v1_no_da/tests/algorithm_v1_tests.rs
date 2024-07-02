use crate::v1_no_da::AlgorithmV0;

#[test]
fn calculate__gives_static_value() {
    // given
    let value = 100;
    let algorithm = AlgorithmV0 {
        new_exec_price: value,
    };

    // when
    let actual = algorithm.calculate();

    // then
    let expected = value;
    assert_eq!(expected, actual);
}
