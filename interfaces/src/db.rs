use std::io::ErrorKind;

use fuel_vm::prelude::InterpreterError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("error performing binary serialization")]
    Codec,
    #[error("Failed to initialize chain")]
    ChainAlreadyInitialized,
    #[error("Chain is not yet initialized")]
    ChainUninitialized,
    #[error("Invalid database version")]
    InvalidDatabaseVersion,
    #[error("error occurred in the underlying datastore `{0}`")]
    DatabaseError(Box<dyn std::error::Error + Send + Sync>),
}

impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self {
        std::io::Error::new(ErrorKind::Other, e)
    }
}

#[derive(Debug, Error)]
pub enum KvStoreError {
    #[error("generic error occurred")]
    Error(Box<dyn std::error::Error + Send + Sync>),
    #[error("resource not found")]
    NotFound,
}

impl From<Error> for KvStoreError {
    fn from(e: Error) -> Self {
        KvStoreError::Error(Box::new(e))
    }
}

impl From<KvStoreError> for Error {
    fn from(e: KvStoreError) -> Self {
        Error::DatabaseError(Box::new(e))
    }
}

impl From<KvStoreError> for std::io::Error {
    fn from(e: KvStoreError) -> Self {
        std::io::Error::new(ErrorKind::Other, e)
    }
}

impl From<Error> for InterpreterError {
    fn from(e: Error) -> Self {
        InterpreterError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

impl From<KvStoreError> for InterpreterError {
    fn from(e: KvStoreError) -> Self {
        InterpreterError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

#[cfg(any(test_helpers))]
pub struct DummyDB {}

#[cfg(any(test, feature = "test_helpers"))]
pub mod helpers {

    use core::str::FromStr;
    use std::sync::Arc;

    use fuel_asm::Opcode;
    use fuel_storage::Storage;
    use fuel_tx::{
        Address, Bytes32, ContractId, Input, Metadata, Output, Transaction, TxId, UtxoId,
    };
    use fuel_vm::prelude::Contract;
    use hashbrown::{HashMap, HashSet};

    use crate::txpool::TxPoolDB;

    use super::*;
    #[derive(Clone, Debug)]
    pub struct DummyDB {
        pub tx_hashes: Vec<TxId>,
        pub tx: HashMap<TxId, Arc<Transaction>>,
        pub contract: HashSet<ContractId>,
    }

    impl DummyDB {
        ///
        pub fn dummy_tx(txhash: TxId) -> Transaction {
            // One transfer tx1 depends on db
            // One dependent transfer tx2 on tx1
            // One higher gas_price transfer tx3 from tx1
            // one higher gas_price transfer tx4 then tx2

            let db1 = TxId::from_str(
                "0x00000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap();
            let db2 = TxId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap();
            let _db3 = TxId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000002",
            )
            .unwrap();
            let _db4 = TxId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000003",
            )
            .unwrap();

            let n1 = TxId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000010",
            )
            .unwrap();
            let n2 = TxId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000011",
            )
            .unwrap();
            let n3 = TxId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000012",
            )
            .unwrap();
            let n4 = TxId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000013",
            )
            .unwrap();

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            let tx1 = Transaction::Script {
                gas_price: 10,
                gas_limit: 1_000_000,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(db1, 0),
                    owner: Address::default(),
                    amount: 100,
                    color: Default::default(),
                    witness_index: 0,
                    maturity: 0,
                    predicate: vec![],
                    predicate_data: vec![],
                }],
                outputs: vec![Output::Coin {
                    amount: 100,
                    to: Address::default(),
                    color: Default::default(),
                }],
                witnesses: vec![vec![].into()],
                metadata: Some(Metadata::new(
                    n1,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )),
            };

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            let tx2 = Transaction::Script {
                gas_price: 9,
                gas_limit: 1_000_001,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(n1, 0),
                    owner: Address::default(),
                    amount: 100,
                    color: Default::default(),
                    witness_index: 0,
                    maturity: 0,
                    predicate: vec![],
                    predicate_data: vec![],
                }],
                outputs: vec![Output::Coin {
                    amount: 100,
                    to: Address::default(),
                    color: Default::default(),
                }],
                witnesses: vec![vec![].into()],
                metadata: Some(Metadata::new(
                    n2,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )),
            };

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            // clashes with tx1
            let tx3 = Transaction::Script {
                gas_price: 20, // more then tx1
                gas_limit: 1_000_001,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(db1, 0),
                    owner: Address::default(),
                    amount: 100,
                    color: Default::default(),
                    witness_index: 0,
                    maturity: 0,
                    predicate: vec![],
                    predicate_data: vec![],
                }],
                outputs: vec![Output::Coin {
                    amount: 100,
                    to: Address::default(),
                    color: Default::default(),
                }],
                witnesses: vec![vec![].into()],
                metadata: Some(Metadata::new(
                    n3,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )),
            };

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            // clashes with tx2
            let tx4 = Transaction::Script {
                gas_price: 20, // more then tx1
                gas_limit: 1_000_001,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(db2, 0),
                    owner: Address::default(),
                    amount: 200,
                    color: Default::default(),
                    witness_index: 0,
                    maturity: 0,
                    predicate: vec![],
                    predicate_data: vec![],
                }],
                outputs: vec![Output::Coin {
                    amount: 100,
                    to: Address::default(),
                    color: Default::default(),
                }],
                witnesses: vec![vec![].into()],
                metadata: Some(Metadata::new(
                    n4,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )),
            };

            match txhash {
                _ if n1 == txhash => tx1,
                _ if n2 == txhash => tx2,
                _ if n3 == txhash => tx3,
                _ if n4 == txhash => tx4,
                _ => {
                    panic!("Transaction not found: {:#x?}", txhash);
                }
            }
        }

        pub fn filled() -> Self {
            let tx_ids = [
                TxId::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
                TxId::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000000001",
                )
                .unwrap(),
                TxId::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000000002",
                )
                .unwrap(),
                TxId::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000000003",
                )
                .unwrap(),
            ];

            let fun = |mut t: Transaction| {
                t.precompute_metadata();
                t
            };

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            // dumy tx used for tests
            let mut txs = vec![
                fun(Transaction::script(
                    10,
                    1000,
                    0,
                    script.clone(),
                    Vec::new(),
                    vec![],
                    vec![Output::Coin {
                        amount: 100,
                        to: Address::default(),
                        color: Default::default(),
                    }],
                    vec![],
                )),
                fun(Transaction::script(
                    10,
                    1000,
                    0,
                    script.clone(),
                    Vec::new(),
                    vec![],
                    vec![Output::Coin {
                        amount: 200,
                        to: Address::default(),
                        color: Default::default(),
                    }],
                    vec![],
                )),
                fun(Transaction::script(
                    10,
                    1000,
                    0,
                    script,
                    Vec::new(),
                    vec![],
                    vec![Output::Coin {
                        amount: 300,
                        to: Address::default(),
                        color: Default::default(),
                    }],
                    vec![],
                )),
                fun(Transaction::script(
                    10,
                    1000,
                    0,
                    Vec::new(),
                    Vec::new(),
                    vec![],
                    vec![Output::Coin {
                        amount: 400,
                        to: Address::default(),
                        color: Default::default(),
                    }],
                    vec![],
                )),
            ];

            for (i, tx) in txs.iter_mut().enumerate() {
                let metadata = match tx {
                    Transaction::Create { metadata, .. } => metadata,
                    Transaction::Script { metadata, .. } => metadata,
                };
                *metadata = Some(Metadata::new(
                    tx_ids[i],
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                ));
            }

            Self {
                tx_hashes: txs.iter().map(|t| t.id()).collect(),
                tx: HashMap::from_iter(txs.into_iter().map(|tx| (tx.id(), Arc::new(tx)))),
                contract: HashSet::new(),
            }
        }

        pub fn tx(&self, n: usize) -> Arc<Transaction> {
            self.tx.get(self.tx_hashes.get(n).unwrap()).unwrap().clone()
        }
    }

    impl Storage<Bytes32, Transaction> for DummyDB {
        type Error = KvStoreError;

        fn insert(
            &mut self,
            _key: &Bytes32,
            _value: &Transaction,
        ) -> Result<Option<Transaction>, Self::Error> {
            unreachable!()
        }

        fn remove(&mut self, _key: &Bytes32) -> Result<Option<Transaction>, Self::Error> {
            unreachable!()
        }

        fn get<'a>(
            &'a self,
            _key: &Bytes32,
        ) -> Result<Option<std::borrow::Cow<'a, Transaction>>, Self::Error> {
            unreachable!()
        }

        fn contains_key(&self, _key: &Bytes32) -> Result<bool, Self::Error> {
            unreachable!()
        }
    }
    impl Storage<ContractId, Contract> for DummyDB {
        type Error = crate::db::Error;

        fn insert(
            &mut self,
            _key: &ContractId,
            _value: &Contract,
        ) -> Result<Option<Contract>, Self::Error> {
            unreachable!()
        }

        fn remove(&mut self, _key: &ContractId) -> Result<Option<Contract>, Self::Error> {
            unreachable!()
        }

        fn get<'a>(
            &'a self,
            _key: &ContractId,
        ) -> Result<Option<std::borrow::Cow<'a, Contract>>, Self::Error> {
            unreachable!()
        }

        fn contains_key(&self, _key: &ContractId) -> Result<bool, Self::Error> {
            unreachable!()
        }
    }

    impl TxPoolDB for DummyDB {
        fn transaction(&self, tx_hash: TxId) -> Option<Arc<Transaction>> {
            self.tx.get(&tx_hash).cloned()
        }

        fn contract_exist(&self, contract_id: ContractId) -> bool {
            self.contract.get(&contract_id).is_some()
        }
    }
}
