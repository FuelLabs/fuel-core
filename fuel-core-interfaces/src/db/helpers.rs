use async_trait::async_trait;
use chrono::Utc;
use lazy_static::lazy_static;
use parking_lot::Mutex;

// constants
pub const TX1_GAS_PRICE: u64 = 10u64;
pub const TX1_BYTE_PRICE: u64 = 5u64;
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
use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use fuel_asm::Opcode;
use fuel_storage::Storage;
use fuel_tx::{Address, Bytes32, ContractId, Input, Metadata, Output, Transaction, TxId, UtxoId};
use fuel_vm::prelude::Contract;
use std::collections::{HashMap, HashSet};

use crate::{
    model::{
        BlockHeight, Coin, CoinStatus, DaBlockHeight, DepositCoin, FuelBlock, FuelBlockConsensus,
        FuelBlockHeader, SealedFuelBlock, ValidatorStake,
    },
    relayer::{RelayerDb, StakingDiff},
    txpool::TxPoolDb,
};

use super::*;

#[derive(Clone, Debug)]
pub struct DummyDb {
    /// wrapped data.
    pub data: Arc<Mutex<Data>>,
}

#[derive(Clone, Debug)]
pub struct Data {
    /// Used for Storage<Address, (u64, Option<Address>)>
    /// Contains test validator set: <validator_address, (stake and consensus_key)>;
    pub validators: HashMap<Address, (ValidatorStake, Option<Address>)>,
    /// variable for current validators height, at height our validator set is
    pub validators_height: DaBlockHeight,
    /// Used for Storage<DaBlockHeight, StakingDiff>
    /// Contains test data for staking changed that is done inside DaBlockHeight.
    /// Staking diff changes can be validator un/registration and un/delegations.
    pub staking_diffs: BTreeMap<DaBlockHeight, StakingDiff>,
    /// index of blocks where delegation happened.
    pub delegator_index: BTreeMap<Address, Vec<DaBlockHeight>>,
    /// blocks that have consensus votes/
    pub sealed_blocks: HashMap<BlockHeight, Arc<SealedFuelBlock>>,
    /// variable for best fuel block height
    pub chain_height: BlockHeight,
    pub finalized_da_height: DaBlockHeight,
    pub tx_hashes: Vec<TxId>,
    /// Dummy transactions
    pub tx: HashMap<TxId, Arc<Transaction>>,
    /// Dummy coins
    pub coins: HashMap<UtxoId, Coin>,
    /// Dummy contracts
    pub contract: HashSet<ContractId>,
    /// Dummy deposit coins.
    pub deposit_coin: HashMap<Bytes32, DepositCoin>,
    /// variable for last commited and finalized fuel height
    pub last_commited_finalized_fuel_height: BlockHeight,
}

impl DummyDb {
    pub fn insert_sealed_block(&self, sealed_block: Arc<SealedFuelBlock>) {
        self.data
            .lock()
            .sealed_blocks
            .insert(sealed_block.header.height, sealed_block);
    }
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
            gas_price: TX1_GAS_PRICE,
            gas_limit: 1_000_000,
            byte_price: TX1_BYTE_PRICE,
            maturity: 0,
            receipts_root: Default::default(),
            script,
            script_data: vec![],
            inputs: vec![Input::CoinSigned {
                utxo_id: UtxoId::new(*TX_ID_DB1, 0),
                owner: Address::default(),
                amount: 100,
                asset_id: Default::default(),
                witness_index: 0,
                maturity: 0,
            }],
            outputs: vec![
                Output::Coin {
                    amount: 100,
                    to: Address::default(),
                    asset_id: Default::default(),
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
            inputs: vec![Input::CoinSigned {
                utxo_id: UtxoId::new(*TX_ID_DB1, 0),
                owner: Address::default(),
                amount: 100,
                asset_id: Default::default(),
                witness_index: 0,
                maturity: 0,
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
            inputs: vec![Input::CoinSigned {
                utxo_id: UtxoId::new(*TX_ID1, 0),
                owner: Address::default(),
                amount: 100,
                asset_id: Default::default(),
                witness_index: 0,
                maturity: 0,
            }],
            outputs: vec![Output::Coin {
                amount: 100,
                to: Address::default(),
                asset_id: Default::default(),
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
            inputs: vec![Input::CoinSigned {
                utxo_id: UtxoId::new(*TX_ID1, 0),
                owner: Address::default(),
                amount: 100,
                asset_id: Default::default(),
                witness_index: 0,
                maturity: 0,
            }],
            outputs: vec![
                Output::Coin {
                    amount: 100,
                    to: Address::default(),
                    asset_id: Default::default(),
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
            inputs: vec![Input::CoinSigned {
                utxo_id: UtxoId::new(*TX_ID_DB1, 0),
                owner: Address::default(),
                amount: 100,
                asset_id: Default::default(),
                witness_index: 0,
                maturity: 0,
            }],
            outputs: vec![Output::Coin {
                amount: 100,
                to: Address::default(),
                asset_id: Default::default(),
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
            inputs: vec![Input::CoinSigned {
                utxo_id: UtxoId::new(*TX_ID_DB2, 0),
                owner: Address::default(),
                amount: 200,
                asset_id: Default::default(),
                witness_index: 0,
                maturity: 0,
            }],
            outputs: vec![Output::Coin {
                amount: 100,
                to: Address::default(),
                asset_id: Default::default(),
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
                    asset_id: Default::default(),
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
                    asset_id: Default::default(),
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
                    asset_id: Default::default(),
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
                    asset_id: Default::default(),
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
                    asset_id: Default::default(),
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

        let block_created = BlockHeight::from(0u64);
        let mut coins = HashMap::new();
        for tx in txs.iter() {
            for (output_index, output) in tx.outputs().iter().enumerate() {
                let utxo_id = UtxoId::new(tx.id(), output_index as u8);
                let coin = match output {
                    Output::Coin {
                        amount,
                        asset_id,
                        to,
                    } => Coin {
                        owner: *to,
                        amount: *amount,
                        asset_id: *asset_id,
                        maturity: 0u32.into(),
                        status: CoinStatus::Unspent,
                        block_created,
                    },
                    Output::Change {
                        to,
                        asset_id,
                        amount,
                    } => Coin {
                        owner: *to,
                        amount: *amount,
                        asset_id: *asset_id,
                        maturity: 0u32.into(),
                        status: CoinStatus::Unspent,
                        block_created,
                    },
                    Output::Variable {
                        to,
                        asset_id,
                        amount,
                    } => Coin {
                        owner: *to,
                        amount: *amount,
                        asset_id: *asset_id,
                        maturity: 0u32.into(),
                        status: CoinStatus::Unspent,
                        block_created,
                    },
                    _ => continue,
                };
                coins.insert(utxo_id, coin);
            }
        }

        let block = SealedFuelBlock {
            block: FuelBlock {
                header: FuelBlockHeader {
                    number: BlockHeight::from(2u64),
                    time: Utc::now(),
                    ..Default::default()
                },
                transactions: Vec::new(),
            },
            consensus: FuelBlockConsensus {
                required_stake: 10,
                ..Default::default()
            },
        };

        let mut block1 = block.clone();
        block1.block.header.height = 1u64.into();
        let mut block2 = block.clone();
        block2.block.header.height = 2u64.into();
        let mut block3 = block.clone();
        block3.block.header.height = 3u64.into();
        let mut block4 = block;
        block4.block.header.height = 4u64.into();

        let data = Data {
            tx_hashes: txs.iter().map(|t| t.id()).collect(),
            tx: HashMap::from_iter(txs.into_iter().map(|tx| (tx.id(), Arc::new(tx)))),
            coins,
            contract: HashSet::new(),
            deposit_coin: HashMap::new(),
            chain_height: BlockHeight::from(0u64),
            validators_height: 0,
            finalized_da_height: 0,
            sealed_blocks: HashMap::from([
                (1u64.into(), Arc::new(block1)),
                (2u64.into(), Arc::new(block2)),
                (3u64.into(), Arc::new(block3)),
                (4u64.into(), Arc::new(block4)),
            ]),
            validators: HashMap::new(),
            staking_diffs: BTreeMap::new(),
            delegator_index: BTreeMap::new(),
            last_commited_finalized_fuel_height: BlockHeight::from(0u64),
        };

        Self {
            data: Arc::new(Mutex::new(data)),
        }
    }

    pub fn tx(&self, n: usize) -> Arc<Transaction> {
        let data = self.data.lock();
        data.tx.get(data.tx_hashes.get(n).unwrap()).unwrap().clone()
    }
}

impl Storage<UtxoId, Coin> for DummyDb {
    type Error = KvStoreError;

    fn insert(&mut self, _key: &UtxoId, _value: &Coin) -> Result<Option<Coin>, Self::Error> {
        unreachable!()
    }

    fn remove(&mut self, _key: &UtxoId) -> Result<Option<Coin>, Self::Error> {
        unreachable!()
    }

    fn get<'a>(&'a self, _key: &UtxoId) -> Result<Option<std::borrow::Cow<'a, Coin>>, Self::Error> {
        unreachable!()
    }

    fn contains_key(&self, _key: &UtxoId) -> Result<bool, Self::Error> {
        unreachable!()
    }
}

impl Storage<Bytes32, Transaction> for DummyDb {
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

impl Storage<ContractId, Contract> for DummyDb {
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

impl TxPoolDb for DummyDb {
    fn utxo(&self, utxo_id: &UtxoId) -> Result<Option<Coin>, KvStoreError> {
        Ok(self.data.lock().coins.get(utxo_id).cloned())
    }

    fn contract_exist(&self, contract_id: ContractId) -> Result<bool, Error> {
        Ok(self.data.lock().contract.get(&contract_id).is_some())
    }
}

// token deposit. Used by relayer.
impl Storage<Bytes32, DepositCoin> for DummyDb {
    type Error = crate::db::KvStoreError;

    fn insert(
        &mut self,
        key: &Bytes32,
        value: &DepositCoin,
    ) -> Result<Option<DepositCoin>, Self::Error> {
        Ok(self.data.lock().deposit_coin.insert(*key, value.clone()))
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

// delegates index. Used by relayer.
impl Storage<Address, Vec<DaBlockHeight>> for DummyDb {
    type Error = crate::db::KvStoreError;

    fn insert(
        &mut self,
        key: &Address,
        value: &Vec<DaBlockHeight>,
    ) -> Result<Option<Vec<DaBlockHeight>>, Self::Error> {
        Ok(self.data.lock().delegator_index.insert(*key, value.clone()))
    }

    fn remove(&mut self, _key: &Address) -> Result<Option<Vec<DaBlockHeight>>, Self::Error> {
        unreachable!()
    }

    fn get<'a>(
        &'a self,
        key: &Address,
    ) -> Result<Option<std::borrow::Cow<'a, Vec<DaBlockHeight>>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .delegator_index
            .get(key)
            .map(|i| Cow::Owned(i.clone())))
    }

    fn contains_key(&self, _key: &Address) -> Result<bool, Self::Error> {
        unreachable!()
    }
}

// Validator set. Used by relayer.
impl Storage<Address, (ValidatorStake, Option<Address>)> for DummyDb {
    type Error = crate::db::KvStoreError;

    fn insert(
        &mut self,
        key: &Address,
        value: &(ValidatorStake, Option<Address>),
    ) -> Result<Option<(ValidatorStake, Option<Address>)>, Self::Error> {
        Ok(self.data.lock().validators.insert(*key, *value))
    }

    fn remove(
        &mut self,
        _key: &Address,
    ) -> Result<Option<(ValidatorStake, Option<Address>)>, Self::Error> {
        unreachable!()
    }

    fn get<'a>(
        &'a self,
        key: &Address,
    ) -> Result<Option<std::borrow::Cow<'a, (ValidatorStake, Option<Address>)>>, Self::Error> {
        Ok(self.data.lock().validators.get(key).map(|i| Cow::Owned(*i)))
    }

    fn contains_key(&self, _key: &Address) -> Result<bool, Self::Error> {
        unreachable!()
    }
}

// Staking diff. Used by relayer.
impl Storage<DaBlockHeight, StakingDiff> for DummyDb {
    type Error = crate::db::KvStoreError;

    fn insert(
        &mut self,
        key: &DaBlockHeight,
        value: &StakingDiff,
    ) -> Result<Option<StakingDiff>, Self::Error> {
        Ok(self.data.lock().staking_diffs.insert(*key, value.clone()))
    }

    fn remove(&mut self, _key: &DaBlockHeight) -> Result<Option<StakingDiff>, Self::Error> {
        unreachable!()
    }

    fn get<'a>(
        &'a self,
        key: &DaBlockHeight,
    ) -> Result<Option<std::borrow::Cow<'a, StakingDiff>>, Self::Error> {
        Ok(self
            .data
            .lock()
            .staking_diffs
            .get(key)
            .map(|i| Cow::Owned(i.clone())))
    }

    fn contains_key(&self, _key: &DaBlockHeight) -> Result<bool, Self::Error> {
        unreachable!()
    }
}

#[async_trait]
impl RelayerDb for DummyDb {
    async fn get_validators(&self) -> HashMap<Address, (ValidatorStake, Option<Address>)> {
        self.data.lock().validators.clone()
    }

    async fn set_validators_da_height(&self, block: DaBlockHeight) {
        self.data.lock().validators_height = block;
    }

    async fn get_validators_da_height(&self) -> DaBlockHeight {
        self.data.lock().validators_height
    }

    async fn get_staking_diffs(
        &self,
        from_da_height: DaBlockHeight,
        to_da_height: Option<DaBlockHeight>,
    ) -> Vec<(DaBlockHeight, StakingDiff)> {
        let mut out = Vec::new();
        let diffs = &self.data.lock().staking_diffs;
        // in BTreeMap iteration are done on sorted items.
        for (block, diff) in diffs {
            if *block >= from_da_height {
                if let Some(end_block) = to_da_height {
                    if *block > end_block {
                        break;
                    }
                }
                out.push((*block, diff.clone()));
            }
        }
        out
    }

    async fn get_chain_height(&self) -> BlockHeight {
        self.data.lock().chain_height
    }

    async fn get_sealed_block(&self, height: BlockHeight) -> Option<Arc<SealedFuelBlock>> {
        self.data.lock().sealed_blocks.get(&height).cloned()
    }

    async fn set_finalized_da_height(&self, height: DaBlockHeight) {
        self.data.lock().finalized_da_height = height;
    }

    async fn get_finalized_da_height(&self) -> DaBlockHeight {
        self.data.lock().finalized_da_height
    }

    async fn get_last_commited_finalized_fuel_height(&self) -> BlockHeight {
        self.data.lock().last_commited_finalized_fuel_height
    }

    async fn set_last_commited_finalized_fuel_height(&self, block_height: BlockHeight) {
        self.data.lock().last_commited_finalized_fuel_height = block_height;
    }
}
