pub mod consts;
pub mod crypto;
pub mod interpreter;

pub mod prelude {
    pub use crate::interpreter::{Call, CallFrame, Contract, ExecuteError, Interpreter, LogEvent, MemoryRange};
    pub use fuel_asm::{Immediate06, Immediate12, Immediate18, Immediate24, Opcode, RegisterId, Word};
    pub use fuel_tx::{
        bytes::{Deserializable, SerializableVec, SizedBytes},
        Address, Color, ContractAddress, Hash, Input, Output, Salt, Transaction, ValidationError, Witness,
    };
}

mod state {
    use serde::de::DeserializeOwned;
    use serde::Serialize;

    pub trait KeyValueStore {
        fn get<K: AsRef<u8>, V: Serialize>(&self, key: K) -> Option<V>;
        fn put<K: AsRef<u8>, V: DeserializeOwned>(&mut self, key: K, value: V) -> Option<V>;
        fn delete<K: AsRef<u8>, V: DeserializeOwned>(&mut self, key: K) -> Option<V>;
        fn exists<K: AsRef<u8>>(&self, key: K) -> bool;
    }

    pub trait TransactionalProxy {
        fn commit(&mut self);
    }

    pub trait Transactional {
        /// The proxy is used to provide a transactional view of state that merges
        /// uncommitted changes with the original source
        type StoreProxy: KeyValueStore + TransactionalProxy;

        fn make_proxy(&self) -> Self::StoreProxy;

        fn transaction<F, S, R, E>(&mut self, f: F) -> TransactionResult<R, E>
        where
            F: FnOnce(&mut S) -> TransactionResult<R, E>,
            S: KeyValueStore,
        {
            let mut proxy = self.make_proxy();
            let result = f(&mut proxy);
            if result.is_ok() {
                proxy.commit();
            }
            result
        }
    }

    pub type TransactionResult<T, E> = Result<T, TransactionError<E>>;

    pub enum TransactionError<T> {
        Aborted(T),
    }
}
