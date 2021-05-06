use fuel_vm_rust::prelude::*;

pub fn dummy_tx() -> Transaction {
    Transaction::create(1, 1000000, 10, 0, 0, [0u8; 32], vec![], vec![], vec![], vec![])
}
