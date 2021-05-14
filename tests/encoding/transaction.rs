use fuel_vm_rust::prelude::*;

use std::fmt;
use std::io::{self, Read, Write};

fn assert_encoding_correct<T>(data: &[T])
where
    T: Read + Write + fmt::Debug + Clone + PartialEq,
{
    let mut buffer;

    for data in data.iter() {
        let mut d = data.clone();
        let mut d_p = data.clone();

        buffer = vec![0u8; 1024];
        let read_size = d.read(buffer.as_mut_slice()).expect("Failed to read");
        let write_size = d_p.write(buffer.as_slice()).expect("Failed to write");

        // Simple RW assertion
        assert_eq!(d, d_p);
        assert_eq!(read_size, write_size);

        buffer = vec![0u8; read_size];

        // Minimum size buffer assertion
        d.read(buffer.as_mut_slice()).expect("Failed to read");
        d_p.write(buffer.as_slice()).expect("Failed to write");
        assert_eq!(d, d_p);

        // No panic assertion
        loop {
            buffer.pop();

            let err = d
                .read(buffer.as_mut_slice())
                .err()
                .expect("Insufficient buffer should fail!");
            assert_eq!(io::ErrorKind::UnexpectedEof, err.kind());

            let err = d_p
                .write(buffer.as_slice())
                .err()
                .expect("Insufficient buffer should fail!");
            assert_eq!(io::ErrorKind::UnexpectedEof, err.kind());

            if buffer.is_empty() {
                break;
            }
        }
    }
}

#[test]
fn witness() {
    assert_encoding_correct(&[Witness::from(vec![0xef]), Witness::from(vec![])]);
}

#[test]
fn input() {
    assert_encoding_correct(&[
        Input::coin(
            [0xaa; 32],
            [0xbb; 32],
            Word::MAX,
            [0xcc; 32],
            0xff,
            Word::MAX >> 1,
            vec![0xdd; 50],
            vec![0xee; 23],
        ),
        Input::coin(
            [0xaa; 32],
            [0xbb; 32],
            Word::MAX,
            [0xcc; 32],
            0xff,
            Word::MAX >> 1,
            vec![],
            vec![0xee; 23],
        ),
        Input::coin(
            [0xaa; 32],
            [0xbb; 32],
            Word::MAX,
            [0xcc; 32],
            0xff,
            Word::MAX >> 1,
            vec![0xdd; 50],
            vec![],
        ),
        Input::coin(
            [0xaa; 32],
            [0xbb; 32],
            Word::MAX,
            [0xcc; 32],
            0xff,
            Word::MAX >> 1,
            vec![],
            vec![],
        ),
        Input::contract([0xaa; 32], [0xbb; 32], [0xcc; 32], [0xdd; 32]),
    ]);
}

#[test]
fn output() {
    assert_encoding_correct(&[
        Output::coin([0xaa; 32], Word::MAX >> 1, [0xbb; 32]),
        Output::contract(0xaa, [0xbb; 32], [0xcc; 32]),
        Output::withdrawal([0xaa; 32], Word::MAX >> 1, [0xbb; 32]),
        Output::change([0xaa; 32], Word::MAX >> 1, [0xbb; 32]),
        Output::variable([0xaa; 32], Word::MAX >> 1, [0xbb; 32]),
        Output::contract_created([0xaa; 32]),
    ]);
}

#[test]
fn transaction() {
    let i = Input::contract([0xaa; 32], [0xbb; 32], [0xcc; 32], [0xdd; 32]);
    let o = Output::coin([0xaa; 32], Word::MAX >> 1, [0xbb; 32]);
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
            [0xdd; 32],
            vec![[0xce; 32]],
            vec![i.clone()],
            vec![o.clone()],
            vec![w.clone()],
        ),
        Transaction::create(
            Word::MAX >> 1,
            Word::MAX >> 2,
            Word::MAX >> 3,
            0xba,
            [0xdd; 32],
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
            [0xdd; 32],
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
            [0xdd; 32],
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
            [0xdd; 32],
            vec![],
            vec![],
            vec![],
            vec![],
        ),
    ]);
}
