use super::{Contract, ExecuteError, Interpreter};
use crate::data::Storage;

use fuel_asm::Word;
use fuel_tx::bytes::Deserializable;
use fuel_tx::{Color, ContractAddress, Transaction};

impl<S> Interpreter<S>
where
    S: Storage<ContractAddress, Contract> + Storage<Color, Word>,
{
    pub fn execute_tx_bytes(storage: S, bytes: &[u8]) -> Result<Self, ExecuteError> {
        let tx = Transaction::from_bytes(bytes)?;

        Self::execute_tx(storage, tx)
    }

    pub fn execute_tx(storage: S, tx: Transaction) -> Result<Self, ExecuteError> {
        let mut vm = Interpreter::with_storage(storage);

        vm.init(tx)?;
        vm.run()?;

        Ok(vm)
    }
}
