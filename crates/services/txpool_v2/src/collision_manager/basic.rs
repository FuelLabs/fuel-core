use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    fmt::Debug,
    hash::Hash,
};

use fuel_core_types::{
    fuel_tx::{
        BlobId,
        ContractId,
        Input,
        Output,
        TxId,
        UtxoId,
        field::BlobId as _,
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            contract::Contract as ContractInput,
            message::{
                MessageCoinPredicate,
                MessageCoinSigned,
                MessageDataPredicate,
                MessageDataSigned,
            },
        },
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};

use crate::{
    error::{
        CollisionReason,
        Error,
        InputValidationError,
    },
    storage::StorageData,
};

use super::{
    CollisionManager,
    Collisions,
};

#[cfg(test)]
use fuel_core_types::services::txpool::ArcPoolTx;

pub struct BasicCollisionManager<StorageIndex> {
    /// Message -> Transaction that currently use the Message
    messages_spenders: HashMap<Nonce, StorageIndex>,
    /// Coins -> Transaction that currently use the UTXO
    coins_spenders: BTreeMap<UtxoId, StorageIndex>,
    /// Contract -> Transaction that currently create the contract
    contracts_creators: HashMap<ContractId, StorageIndex>,
    /// Contract -> Transactions (by TxId) that currently use the contract as an input.
    /// Symmetric to `contracts_creators`; used to evict dependents during rollback.
    contract_users: HashMap<ContractId, Vec<TxId>>,
    /// Blob -> Transaction that currently create the blob
    blobs_users: HashMap<BlobId, StorageIndex>,
}

impl<StorageIndex> BasicCollisionManager<StorageIndex> {
    pub fn new() -> Self {
        Self {
            messages_spenders: HashMap::new(),
            coins_spenders: BTreeMap::new(),
            contracts_creators: HashMap::new(),
            contract_users: HashMap::new(),
            blobs_users: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.messages_spenders.is_empty()
            && self.coins_spenders.is_empty()
            && self.contracts_creators.is_empty()
            && self.contract_users.is_empty()
            && self.blobs_users.is_empty()
    }

    #[cfg(test)]
    /// Expected transactions are the transactions that must be populate all elements present in the collision manager.
    /// This function will check if all elements present in the collision manager are present in the expected transactions and vice versa.
    pub(crate) fn assert_integrity(&self, expected_txs: &[ArcPoolTx]) {
        use std::ops::Deref;

        let mut message_spenders = HashMap::new();
        let mut coins_spenders = BTreeMap::new();
        let mut contracts_creators = HashMap::new();
        let mut contract_users: HashMap<ContractId, Vec<TxId>> = HashMap::new();
        let mut blobs_users = HashMap::new();
        for tx in expected_txs {
            if let PoolTransaction::Blob(checked_tx, _) = tx.deref() {
                let blob_id = checked_tx.transaction().blob_id();
                blobs_users.insert(*blob_id, tx.id());
            }
            for input in tx.inputs() {
                match input {
                    Input::CoinSigned(CoinSigned { utxo_id, .. })
                    | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                        coins_spenders.insert(*utxo_id, tx.id());
                    }
                    Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                    | Input::MessageCoinPredicate(MessageCoinPredicate {
                        nonce, ..
                    })
                    | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                    | Input::MessageDataPredicate(MessageDataPredicate {
                        nonce, ..
                    }) => {
                        message_spenders.insert(*nonce, tx.id());
                    }
                    Input::Contract(ContractInput { contract_id, .. }) => {
                        contract_users
                            .entry(*contract_id)
                            .or_default()
                            .push(tx.id());
                    }
                }
            }
            for output in tx.outputs() {
                if let Output::ContractCreated { contract_id, .. } = output {
                    contracts_creators.insert(*contract_id, tx.id());
                }
            }
        }
        for nonce in self.messages_spenders.keys() {
            message_spenders.remove(nonce).unwrap_or_else(|| panic!(
                "A message ({}) spender is present on the collision manager that shouldn't be there.",
                nonce
            ));
        }
        assert!(
            message_spenders.is_empty(),
            "Some message spenders are missing from the collision manager: {:?}",
            message_spenders
        );
        for utxo_id in self.coins_spenders.keys() {
            coins_spenders.remove(utxo_id).unwrap_or_else(|| panic!(
                "A coin ({}) spender is present on the collision manager that shouldn't be there.",
                utxo_id
            ));
        }
        assert!(
            coins_spenders.is_empty(),
            "Some coin senders are missing from the collision manager: {:?}",
            coins_spenders
        );
        for contract_id in self.contracts_creators.keys() {
            contracts_creators.remove(contract_id).unwrap_or_else(|| panic!(
                "A contract ({}) creator is present on the collision manager that shouldn't be there.",
                contract_id
            ));
        }
        assert!(
            contracts_creators.is_empty(),
            "Some contract creators are missing from the collision manager: {:?}",
            contracts_creators
        );
        for (contract_id, users) in &self.contract_users {
            let expected = contract_users.remove(contract_id).unwrap_or_else(|| panic!(
                "A contract ({}) user list is present on the collision manager that shouldn't be there.",
                contract_id
            ));
            let mut actual_sorted = users.clone();
            actual_sorted.sort();
            let mut expected_sorted = expected;
            expected_sorted.sort();
            assert_eq!(
                actual_sorted, expected_sorted,
                "contract_users mismatch for contract {}",
                contract_id
            );
        }
        assert!(
            contract_users.is_empty(),
            "Some contract users are missing from the collision manager: {:?}",
            contract_users
        );
    }
}

impl<StorageIndex> Default for BasicCollisionManager<StorageIndex> {
    fn default() -> Self {
        Self::new()
    }
}

impl<StorageIndex> CollisionManager for BasicCollisionManager<StorageIndex>
where
    StorageIndex: Copy + Debug + Hash + PartialEq + Eq,
{
    type StorageIndex = StorageIndex;

    fn get_coins_spenders(&self, tx_creator_id: &TxId) -> Vec<Self::StorageIndex> {
        self.coins_spenders
            .range(UtxoId::new(*tx_creator_id, 0)..UtxoId::new(*tx_creator_id, u16::MAX))
            .map(|(_, storage_id)| *storage_id)
            .collect()
    }

    fn get_contract_users(&self, contract_id: &ContractId) -> Vec<TxId> {
        self.contract_users
            .get(contract_id)
            .cloned()
            .unwrap_or_default()
    }

    fn contract_created_in_pool(&self, contract_id: &ContractId) -> bool {
        self.contracts_creators.contains_key(contract_id)
    }

    fn find_collisions(
        &self,
        transaction: &PoolTransaction,
    ) -> Result<Collisions<Self::StorageIndex>, Error> {
        let mut collisions = HashMap::new();
        if let PoolTransaction::Blob(checked_tx, _) = &transaction {
            let blob_id = checked_tx.transaction().blob_id();
            if let Some(state) = self.blobs_users.get(blob_id) {
                collisions.insert(*state, vec![CollisionReason::Blob(*blob_id)]);
            }
        }
        for input in transaction.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // Check if the utxo is already spent by another transaction in the pool
                    if let Some(storage_id) = self.coins_spenders.get(utxo_id) {
                        let entry = collisions.entry(*storage_id).or_default();
                        entry.push(CollisionReason::Utxo(*utxo_id));
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // Check if the message is already spent by another transaction in the pool
                    if let Some(storage_id) = self.messages_spenders.get(nonce) {
                        let entry = collisions.entry(*storage_id).or_default();
                        entry.push(CollisionReason::Message(*nonce));
                    }
                }
                // No collision for contract inputs
                _ => {}
            }
        }

        for (i, output) in transaction.outputs().iter().enumerate() {
            match output {
                Output::ContractCreated { contract_id, .. } => {
                    // Check if the contract is already created by another transaction in the pool
                    if let Some(storage_id) = self.contracts_creators.get(contract_id) {
                        let entry = collisions.entry(*storage_id).or_default();
                        entry.push(CollisionReason::ContractCreation(*contract_id));
                    }
                }
                Output::Coin { .. } | Output::Change { .. } | Output::Variable { .. } => {
                    let utxo_id = UtxoId::new(
                        transaction.id(),
                        u16::try_from(i)
                            .expect("`Checked` transaction has less than 2^16 outputs"),
                    );

                    if self.coins_spenders.contains_key(&utxo_id) {
                        return Err(Error::InputValidation(
                            InputValidationError::DuplicateTxId(transaction.id()),
                        ));
                    }
                }
                Output::Contract(_) => {}
            }
        }

        Ok(collisions)
    }

    fn on_stored_transaction(
        &mut self,
        storage_id: StorageIndex,
        store_entry: &StorageData,
    ) {
        if let PoolTransaction::Blob(checked_tx, _) = store_entry.transaction.as_ref() {
            let blob_id = checked_tx.transaction().blob_id();
            self.blobs_users.insert(*blob_id, storage_id);
        }
        let tx_id = store_entry.transaction.id();
        for input in store_entry.transaction.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // insert coin
                    self.coins_spenders.insert(*utxo_id, storage_id);
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // insert message
                    self.messages_spenders.insert(*nonce, storage_id);
                }
                Input::Contract(ContractInput { contract_id, .. }) => {
                    self.contract_users
                        .entry(*contract_id)
                        .or_default()
                        .push(tx_id);
                }
            }
        }
        for output in store_entry.transaction.outputs().iter() {
            match output {
                Output::Coin { .. }
                | Output::Change { .. }
                | Output::Variable { .. }
                | Output::Contract(_) => {}
                Output::ContractCreated { contract_id, .. } => {
                    // insert contract
                    self.contracts_creators.insert(*contract_id, storage_id);
                }
            };
        }
    }

    fn on_removed_transaction(&mut self, transaction: &PoolTransaction) {
        if let PoolTransaction::Blob(checked_tx, _) = transaction {
            let blob_id = checked_tx.transaction().blob_id();
            self.blobs_users.remove(blob_id);
        }
        let tx_id = transaction.id();
        for input in transaction.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // remove coin
                    self.coins_spenders.remove(utxo_id);
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // remove message
                    self.messages_spenders.remove(nonce);
                }
                Input::Contract(ContractInput { contract_id, .. }) => {
                    if let Some(users) = self.contract_users.get_mut(contract_id) {
                        users.retain(|id| id != &tx_id);
                        if users.is_empty() {
                            self.contract_users.remove(contract_id);
                        }
                    }
                }
            }
        }
        for output in transaction.outputs().iter() {
            match output {
                Output::Coin { .. }
                | Output::Change { .. }
                | Output::Variable { .. }
                | Output::Contract(_) => {}
                Output::ContractCreated { contract_id, .. } => {
                    // remove contract
                    self.contracts_creators.remove(contract_id);
                }
            };
        }
    }
}
