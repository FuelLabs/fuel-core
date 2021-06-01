use super::assert_encoding_correct;
use fuel_core::prelude::*;

#[test]
fn witness() {
    assert_encoding_correct(&[Witness::from(vec![0xef]), Witness::from(vec![])]);
}

#[test]
fn input() {
    assert_encoding_correct(&[
        Input::coin(
            [0xaa; 32].into(),
            [0xbb; 32].into(),
            Word::MAX,
            [0xcc; 32].into(),
            0xff,
            Word::MAX >> 1,
            vec![0xdd; 50].into(),
            vec![0xee; 23].into(),
        ),
        Input::coin(
            [0xaa; 32].into(),
            [0xbb; 32].into(),
            Word::MAX,
            [0xcc; 32].into(),
            0xff,
            Word::MAX >> 1,
            vec![],
            vec![0xee; 23],
        ),
        Input::coin(
            [0xaa; 32].into(),
            [0xbb; 32].into(),
            Word::MAX,
            [0xcc; 32].into(),
            0xff,
            Word::MAX >> 1,
            vec![0xdd; 50],
            vec![],
        ),
        Input::coin(
            [0xaa; 32].into(),
            [0xbb; 32].into(),
            Word::MAX,
            [0xcc; 32].into(),
            0xff,
            Word::MAX >> 1,
            vec![],
            vec![],
        ),
        Input::contract(
            [0xaa; 32].into(),
            [0xbb; 32].into(),
            [0xcc; 32].into(),
            [0xdd; 32].into(),
        ),
    ]);
}

#[test]
fn output() {
    assert_encoding_correct(&[
        Output::coin([0xaa; 32].into(), Word::MAX >> 1, [0xbb; 32].into()),
        Output::contract(0xaa, [0xbb; 32].into(), [0xcc; 32].into()),
        Output::withdrawal([0xaa; 32].into(), Word::MAX >> 1, [0xbb; 32].into()),
        Output::change([0xaa; 32].into(), Word::MAX >> 1, [0xbb; 32].into()),
        Output::variable([0xaa; 32].into(), Word::MAX >> 1, [0xbb; 32].into()),
        Output::contract_created([0xaa; 32].into()),
    ]);
}

#[test]
fn transaction() {
    let i = Input::contract(
        [0xaa; 32].into(),
        [0xbb; 32].into(),
        [0xcc; 32].into(),
        [0xdd; 32].into(),
    );
    let o = Output::coin([0xaa; 32].into(), Word::MAX >> 1, [0xbb; 32].into());
    let w = Witness::from(vec![0xbf]);

    assert_encoding_correct(&[
        Transaction::script(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            vec![0xfa],
            vec![0xfb, 0xfc],
            vec![i.clone()],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::script(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            vec![],
            vec![0xfb, 0xfc],
            vec![i.clone()],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::script(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            vec![0xfa],
            vec![],
            vec![i.clone()],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::script(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            vec![],
            vec![],
            vec![i.clone()],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::script(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            vec![],
            vec![],
            vec![],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::script(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![w.clone()],
        ),
        Transaction::script(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ),
        Transaction::create(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            0xba,
            [0xdd; 32].into(),
            vec![[0xce; 32].into()],
            vec![i.clone()],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::create(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            0xba,
            [0xdd; 32].into(),
            vec![],
            vec![i.clone()],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::create(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            0xba,
            [0xdd; 32].into(),
            vec![],
            vec![],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::create(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            0xba,
            [0xdd; 32].into(),
            vec![],
            vec![],
            vec![],
            vec![w.clone()],
        ),
        Transaction::create(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            0xba,
            [0xdd; 32].into(),
            vec![],
            vec![],
            vec![],
            vec![],
        ),
    ]);
}
