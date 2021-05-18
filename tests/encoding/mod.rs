use std::fmt;
use std::io::{self, Read, Write};

mod interpreter;
mod transaction;

pub use super::common;

pub fn assert_encoding_correct<T>(data: &[T])
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
