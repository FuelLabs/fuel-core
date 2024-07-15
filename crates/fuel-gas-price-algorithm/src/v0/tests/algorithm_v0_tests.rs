use crate::v0::AlgorithmV0;

#[test]
fn calculate__gives_static_value() {
    // given
    let value = 100;
    let algorithm = AlgorithmV0 {
        new_exec_price: value,
        for_height: 0,
        percentage: 0,
    };

    // when
    let actual = algorithm.calculate();

    // then
    let expected = value;
    assert_eq!(expected, actual);
}

#[test]
fn worst_case__correctly_calculates_value() {
    // given
    let new_exec_price = 1000;
    let for_height = 10;
    let percentage = 10;
    let algorithm = AlgorithmV0 {
        new_exec_price,
        for_height,
        percentage,
    };

    // when
    let delta = 10;
    let target_height = for_height + delta;
    let actual = algorithm.worst_case(target_height);

    // then
    let expected = 2591;
    assert_eq!(expected, actual);
}
#[test]
fn worst_case__same_block_gives_new_exec_price() {
    // given
    let new_exec_price = 1000;
    let for_height = 10;
    let percentage = 10;
    let algorithm = AlgorithmV0 {
        new_exec_price,
        for_height,
        percentage,
    };

    // when
    let delta = 0;
    let target_height = for_height + delta;
    let actual = algorithm.worst_case(target_height);

    // then
    let expected = new_exec_price;
    assert_eq!(expected, actual);
}
