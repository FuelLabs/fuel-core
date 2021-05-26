use super::{ExecuteError, Interpreter};
use fuel_asm::Opcode;
use fuel_tx::bytes::Deserializable;
use fuel_tx::Transaction;

use std::convert::TryFrom;

impl Interpreter {
    pub fn execute_op_bytes(bytes: &[u8]) -> Result<Self, ExecuteError> {
        let iter = bytes.chunks_exact(4).map(|c| {
            let op = <[u8; 4]>::try_from(c).expect("Conversion protected by chunks_exact(4)");
            let op = u32::from_be_bytes(op);

            Opcode::from(op)
        });

        Self::execute_op(iter)
    }

    pub fn execute_op(mut ops: impl Iterator<Item = Opcode>) -> Result<Self, ExecuteError> {
        let mut vm = Interpreter::default();

        vm.init(Transaction::default())?;
        ops.try_for_each(|op| vm.execute(op))?;

        Ok(vm)
    }

    pub fn execute_tx_bytes(bytes: &[u8]) -> Result<Self, ExecuteError> {
        let tx = Transaction::from_bytes(bytes)?;

        Self::execute_tx(tx)
    }

    pub fn execute_tx(tx: Transaction) -> Result<Self, ExecuteError> {
        let mut vm = Interpreter::default();

        vm.init(tx)?;
        vm.run()?;

        Ok(vm)
    }
}
