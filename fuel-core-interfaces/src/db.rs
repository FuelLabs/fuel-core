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

#[cfg(any(test, feature = "test_helpers"))]
pub mod helpers {

    use async_trait::async_trait;
    use lazy_static::lazy_static;
    use parking_lot::Mutex;

    // constants
    lazy_static! {
        pub static ref TX_ID_DB1: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000000")
                .unwrap();
        pub static ref TX_ID_DB2: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        pub static ref TX_ID_DB3: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        pub static ref TX_ID_DB4: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000003")
                .unwrap();
        pub static ref TX_ID1: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000010")
                .unwrap();
        pub static ref TX_ID2: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000011")
                .unwrap();
        pub static ref TX_ID3: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000012")
                .unwrap();
        pub static ref TX_ID4: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000013")
                .unwrap();
        pub static ref TX_ID5: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000014")
                .unwrap();
        pub static ref TX_ID_FAULTY1: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000015")
                .unwrap();
        /// Same as TX_ID2 but it has same ContractId as TX_ID1 so it is not going to be added.
        pub static ref TX_ID_FAULTY2: TxId =
            TxId::from_str("0x0000000000000000000000000000000000000000000000000000000000000016")
                .unwrap();
        pub static ref CONTRACT_ID1: ContractId = ContractId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000100",
        )
        .unwrap();
    }
    //const DB_TX1_HASH: TxId = 0x0000.into();

    use core::str::FromStr;
    use std::{collections::BTreeMap, sync::Arc};

    use fuel_asm::Opcode;
    use fuel_storage::Storage;
    use fuel_tx::{
        Address, Bytes32, ContractId, Input, Metadata, Output, Transaction, TxId, UtxoId,
    };
    use fuel_vm::prelude::Contract;
    use std::collections::{HashMap, HashSet};

    use crate::{
        relayer::{DepositCoin, RelayerDB},
        txpool::TxPoolDb,
    };

    use super::*;
    #[derive(Clone, Debug)]
    pub struct DummyDB {
        pub tx_hashes: Vec<TxId>,
        pub tx: HashMap<TxId, Arc<Transaction>>,
        pub contract: HashSet<ContractId>,
        pub deposit_coin: HashMap<Bytes32, DepositCoin>,
        // relayer relayer
        pub data: Arc<Mutex<Data>>,
    }

    #[derive(Clone, Debug)]
    pub struct Data {
        pub block_height: u64,
        pub current_validator_set: HashMap<Address, u64>,
        pub current_validator_set_block: u64,
        pub validator_set_diff: BTreeMap<u64, HashMap<Address, u64>>,
        pub eth_finalized_block: u64,
        pub fuel_finalized_block: u64,
    }

    impl DummyDB {
        ///
        pub fn dummy_tx(txhash: TxId) -> Transaction {
            // One transfer tx1 depends on db
            // One dependent transfer tx2 on tx1
            // One higher gas_price transfer tx3 from tx1
            // one higher gas_price transfer tx4 then tx2
            // tx5 that depends on tx1 contract
            // tx6 same as tx1 but without coin output

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            let tx1 = Transaction::Script {
                gas_price: 10,
                gas_limit: 1_000_000,
                byte_price: 10,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(*TX_ID_DB1, 0),
                    owner: Address::default(),
                    amount: 100,
                    color: Default::default(),
                    witness_index: 0,
                    maturity: 0,
                    predicate: vec![],
                    predicate_data: vec![],
                }],
                outputs: vec![
                    Output::Coin {
                        amount: 100,
                        to: Address::default(),
                        color: Default::default(),
                    },
                    Output::ContractCreated {
                        contract_id: *CONTRACT_ID1,
                        state_root: Contract::default_state_root(),
                    },
                ],
                witnesses: vec![vec![].into()],
                metadata: Some(Metadata::new(
                    *TX_ID1,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )),
            };

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            let tx1_faulty = Transaction::Script {
                gas_price: 10,
                gas_limit: 1_000_000,
                byte_price: 10,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(*TX_ID_DB1, 0),
                    owner: Address::default(),
                    amount: 100,
                    color: Default::default(),
                    witness_index: 0,
                    maturity: 0,
                    predicate: vec![],
                    predicate_data: vec![],
                }],
                outputs: vec![Output::ContractCreated {
                    contract_id: *CONTRACT_ID1,
                    state_root: Contract::default_state_root(),
                }],
                witnesses: vec![vec![].into()],
                metadata: Some(Metadata::new(
                    *TX_ID1,
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
                byte_price: 9,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(*TX_ID1, 0),
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
                    *TX_ID2,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )),
            };

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            let tx2_faulty = Transaction::Script {
                gas_price: 9,
                gas_limit: 1_000_001,
                byte_price: 9,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(*TX_ID1, 0),
                    owner: Address::default(),
                    amount: 100,
                    color: Default::default(),
                    witness_index: 0,
                    maturity: 0,
                    predicate: vec![],
                    predicate_data: vec![],
                }],
                outputs: vec![
                    Output::Coin {
                        amount: 100,
                        to: Address::default(),
                        color: Default::default(),
                    },
                    Output::ContractCreated {
                        contract_id: *CONTRACT_ID1,
                        state_root: Contract::default_state_root(),
                    },
                ],
                witnesses: vec![vec![].into()],
                metadata: Some(Metadata::new(
                    *TX_ID2,
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
                byte_price: 20,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(*TX_ID_DB1, 0),
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
                    *TX_ID3,
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
                byte_price: 20,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Coin {
                    utxo_id: UtxoId::new(*TX_ID_DB2, 0),
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
                    *TX_ID4,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )),
            };

            let script = Opcode::RET(0x10).to_bytes().to_vec();
            let tx5 = Transaction::Script {
                gas_price: 5, //lower then tx1
                gas_limit: 1_000_000,
                byte_price: 5,
                maturity: 0,
                receipts_root: Default::default(),
                script,
                script_data: vec![],
                inputs: vec![Input::Contract {
                    utxo_id: UtxoId::default(),
                    balance_root: Bytes32::default(),
                    state_root: Bytes32::default(),
                    contract_id: *CONTRACT_ID1,
                }],
                outputs: vec![
                    Output::Coin {
                        amount: 100,
                        to: Address::default(),
                        color: Default::default(),
                    },
                    Output::Contract {
                        input_index: 0,
                        balance_root: Bytes32::default(),
                        state_root: Bytes32::default(),
                    },
                ],
                witnesses: vec![vec![].into()],
                metadata: Some(Metadata::new(
                    *TX_ID5,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )),
            };

            match txhash {
                _ if *TX_ID1 == txhash => tx1,
                _ if *TX_ID2 == txhash => tx2,
                _ if *TX_ID3 == txhash => tx3,
                _ if *TX_ID4 == txhash => tx4,
                _ if *TX_ID5 == txhash => tx5,
                _ if *TX_ID_FAULTY1 == txhash => tx1_faulty,
                _ if *TX_ID_FAULTY2 == txhash => tx2_faulty,
                _ => {
                    panic!("Transaction not found: {:#x?}", txhash);
                }
            }
        }

        pub fn filled() -> Self {
            let tx_ids = [*TX_ID_DB1, *TX_ID_DB2, *TX_ID_DB3, *TX_ID_DB4];

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
                    10,
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
                    10,
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
                    10,
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
                    10,
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

            let data = Data {
                block_height: 0,
                current_validator_set: HashMap::new(),
                current_validator_set_block: 0,
                validator_set_diff: BTreeMap::new(),
                eth_finalized_block: 0,
                fuel_finalized_block: 0,
            };

            Self {
                tx_hashes: txs.iter().map(|t| t.id()).collect(),
                tx: HashMap::from_iter(txs.into_iter().map(|tx| (tx.id(), Arc::new(tx)))),
                contract: HashSet::new(),
                deposit_coin: HashMap::new(),
                data: Arc::new(Mutex::new(data)),
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

    impl TxPoolDb for DummyDB {
        fn transaction(&self, tx_hash: TxId) -> Result<Option<Arc<Transaction>>, KvStoreError> {
            Ok(self.tx.get(&tx_hash).cloned())
        }

        fn contract_exist(&self, contract_id: ContractId) -> Result<bool, Error> {
            Ok(self.contract.get(&contract_id).is_some())
        }
    }

    /* RELAYER needs
    Storage<Bytes32, DepositCoin, Error = KvStoreError> // token deposit
    Storage<Address, u64,Error = KvStoreError> // validator set
    Storage<u64, HashMap<Address, u64>,Error = KvStoreError> // validator set diff
    */

    // token deposit
    impl Storage<Bytes32, DepositCoin> for DummyDB {
        type Error = crate::db::KvStoreError;

        fn insert(
            &mut self,
            key: &Bytes32,
            value: &DepositCoin,
        ) -> Result<Option<DepositCoin>, Self::Error> {
            Ok(self.deposit_coin.insert(*key, value.clone()))
        }

        fn remove(&mut self, _key: &Bytes32) -> Result<Option<DepositCoin>, Self::Error> {
            unreachable!()
        }

        fn get<'a>(
            &'a self,
            _key: &Bytes32,
        ) -> Result<Option<std::borrow::Cow<'a, DepositCoin>>, Self::Error> {
            unreachable!()
        }

        fn contains_key(&self, _key: &Bytes32) -> Result<bool, Self::Error> {
            unreachable!()
        }
    }

    // validator set
    impl Storage<Address, u64> for DummyDB {
        type Error = crate::db::KvStoreError;

        fn insert(&mut self, key: &Address, value: &u64) -> Result<Option<u64>, Self::Error> {
            Ok(self.data.lock().current_validator_set.insert(*key, *value))
        }

        fn remove(&mut self, _key: &Address) -> Result<Option<u64>, Self::Error> {
            unreachable!()
        }

        fn get<'a>(
            &'a self,
            _key: &Address,
        ) -> Result<Option<std::borrow::Cow<'a, u64>>, Self::Error> {
            unreachable!()
        }

        fn contains_key(&self, _key: &Address) -> Result<bool, Self::Error> {
            unreachable!()
        }
    }

    // validator set diff
    impl Storage<u64, HashMap<Address, u64>> for DummyDB {
        type Error = crate::db::KvStoreError;

        fn insert(
            &mut self,
            key: &u64,
            value: &HashMap<Address, u64>,
        ) -> Result<Option<HashMap<Address, u64>>, Self::Error> {
            Ok(self
                .data
                .lock()
                .validator_set_diff
                .insert(*key, value.clone()))
        }

        fn remove(&mut self, _key: &u64) -> Result<Option<HashMap<Address, u64>>, Self::Error> {
            unreachable!()
        }

        fn get<'a>(
            &'a self,
            _key: &u64,
        ) -> Result<Option<std::borrow::Cow<'a, HashMap<Address, u64>>>, Self::Error> {
            unreachable!()
        }

        fn contains_key(&self, _key: &u64) -> Result<bool, Self::Error> {
            unreachable!()
        }
    }

    #[async_trait]
    impl RelayerDB for DummyDB {
        /// get validator set for current fuel block
        async fn current_validator_set(&self) -> HashMap<Address, u64> {
            self.data.lock().current_validator_set.clone()
        }

        /// set last finalized fuel block. In usual case this will be
        async fn set_current_validator_set_block(&self, block: u64) {
            self.data.lock().current_validator_set_block = block;
        }

        /// Assume it is allways set as initialization of database.
        async fn get_current_validator_set_eth_height(&self) -> u64 {
            self.data.lock().current_validator_set_block
        }

        /// get stakes difference between fuel blocks. Return vector of changed (some blocks are not going to have any change)
        async fn get_validator_set_diff(
            &self,
            from_fuel_block: u64,
            to_fuel_block: Option<u64>,
        ) -> Vec<(u64, HashMap<Address, u64>)> {
            let mut out = Vec::new();
            let diffs = &self.data.lock().validator_set_diff;
            // in BTreeMap iteration are done on sorted items.
            for (block, diff) in diffs {
                if from_fuel_block >= *block {
                    out.push((*block, diff.clone()))
                }
                if let Some(end_block) = to_fuel_block {
                    if end_block < *block {
                        break;
                    }
                }
            }
            out
        }

        /// current best block number
        async fn get_block_height(&self) -> u64 {
            self.data.lock().block_height
        }

        /// set newest finalized eth block
        async fn set_eth_finalized_block(&self, block: u64) {
            self.data.lock().eth_finalized_block = block;
        }

        /// assume it is allways set sa initialization of database.
        async fn get_eth_finalized_block(&self) -> u64 {
            self.data.lock().eth_finalized_block
        }

        /// set last finalized fuel block. In usual case this will be
        async fn set_fuel_finalized_block(&self, block: u64) {
            self.data.lock().fuel_finalized_block = block;
        }

        /// Assume it is allways set as initialization of database.
        async fn get_fuel_finalized_block(&self) -> u64 {
            self.data.lock().fuel_finalized_block
        }
    }
}
