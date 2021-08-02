use std::io::Write;
use fuel_tx::Transaction;

extern "C" {
    fn ff_get_transaction(ptr: *const u8, len: usize) -> usize;
}

const BUFFER_SIZE: usize = 4096;
static mut BYTE_BUF: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];

pub fn get_transaction() -> Transaction {
    unsafe {
        let bytes = ff_get_transaction(BYTE_BUF.as_ptr(), BYTE_BUF.len());
        let mut trans = Transaction::default();
        trans.write(&BYTE_BUF[..bytes]).expect("Error deserializing transaction");
        trans
    }
}
