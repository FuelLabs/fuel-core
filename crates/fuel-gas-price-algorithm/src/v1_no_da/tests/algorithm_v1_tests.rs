use crate::v1_no_da::AlgorithmV1NoDA;

#[test]
fn calculate__gives_static_value() {
    // given
    let value = 100;
    let algorithm = AlgorithmV1NoDA {
        new_exec_price: value,
    };

    // when
    let actual = algorithm.calculate();

    // then
    let expected = value;
    assert_eq!(expected, actual);
}
