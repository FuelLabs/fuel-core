use super::{ExecuteError, Interpreter};
use fuel_asm::Opcode;
use fuel_tx::bytes::Deserializable;
use fuel_tx::Transaction;

impl Interpreter {
    pub fn execute_op_bytes(bytes: &[u8]) -> Result<Self, ExecuteError> {
        Self::execute_op(bytes.chunks_exact(Opcode::BYTES_SIZE).map(Opcode::from_bytes_unchecked))
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
