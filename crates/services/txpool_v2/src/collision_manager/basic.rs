use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
};

use fuel_core_types::{
    fuel_tx::{
        field::BlobId as _,
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            message::{
                MessageCoinPredicate,
                MessageCoinSigned,
                MessageDataPredicate,
                MessageDataSigned,
            },
        },
        BlobId,
        ContractId,
        Input,
        Output,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};
use num_rational::Ratio;

use crate::{
    error::{
        CollisionReason,
        Error,
    },
    storage::StorageData,
};

use super::CollisionManager;

pub trait BasicCollisionManagerStorage {
    type StorageIndex: Copy + Debug + Hash + PartialEq + Eq;

    fn get(&self, index: &Self::StorageIndex) -> Result<&StorageData, Error>;
}

pub struct BasicCollisionManager<S: BasicCollisionManagerStorage> {
    /// Message -> Transaction that currently use the Message
    messages_spenders: HashMap<Nonce, S::StorageIndex>,
    /// Coins -> Transaction that currently use the UTXO
    coins_spenders: HashMap<UtxoId, S::StorageIndex>,
    /// Contract -> Transaction that currently create the contract
    contracts_creators: HashMap<ContractId, S::StorageIndex>,
    /// Blob -> Transaction that currently create the blob
    blobs_users: HashMap<BlobId, S::StorageIndex>,
}

impl<S: BasicCollisionManagerStorage> BasicCollisionManager<S> {
    pub fn new() -> Self {
        Self {
            messages_spenders: HashMap::new(),
            coins_spenders: HashMap::new(),
            contracts_creators: HashMap::new(),
            blobs_users: HashMap::new(),
        }
    }
}

impl<S: BasicCollisionManagerStorage> Default for BasicCollisionManager<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: BasicCollisionManagerStorage> BasicCollisionManager<S> {
    fn is_better_than_collision(
        &self,
        tx: &PoolTransaction,
        collision: S::StorageIndex,
        storage: &S,
    ) -> bool {
        let new_tx_ratio = Ratio::new(tx.tip(), tx.max_gas());
        let colliding_tx = storage
            .get(&collision)
            .expect("Transaction always should exist in storage");
        let colliding_tx_ratio = Ratio::new(
            colliding_tx.dependents_cumulative_tip,
            colliding_tx.dependents_cumulative_gas,
        );
        new_tx_ratio > colliding_tx_ratio
    }
}

impl<S: BasicCollisionManagerStorage> CollisionManager for BasicCollisionManager<S> {
    type Storage = S;
    type StorageIndex = S::StorageIndex;

    fn collect_colliding_transactions(
        &self,
        transaction: &PoolTransaction,
    ) -> Result<HashMap<Self::StorageIndex, Vec<CollisionReason>>, Error> {
        let mut collisions = HashMap::new();
        if let PoolTransaction::Blob(checked_tx, _) = transaction {
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

        for output in transaction.outputs() {
            if let Output::ContractCreated { contract_id, .. } = output {
                // Check if the contract is already created by another transaction in the pool
                if let Some(storage_id) = self.contracts_creators.get(contract_id) {
                    let entry = collisions.entry(*storage_id).or_default();
                    entry.push(CollisionReason::ContractCreation(*contract_id));
                }
            }
        }
        Ok(collisions)
    }

    /// Rules:
    // A transaction with dependencies can collide only with one other transaction if it is less worth it
    // A transaction without dependencies can collide with multiple transaction if they are less worth it
    fn can_store_transaction(
        &self,
        transaction: &PoolTransaction,
        has_dependencies: bool,
        colliding_transactions: &HashMap<Self::StorageIndex, Vec<CollisionReason>>,
        storage: &Self::Storage,
    ) -> Result<(), CollisionReason> {
        if colliding_transactions.is_empty() {
            return Ok(());
        }
        if has_dependencies {
            if colliding_transactions.len() > 1 {
                return Err(CollisionReason::MultipleCollisions);
            }
            let (collision, reason) = colliding_transactions.iter().next().unwrap();
            if !self.is_better_than_collision(transaction, *collision, storage) {
                if let Some(reason) = reason.first() {
                    return Err(reason.clone());
                } else {
                    return Err(CollisionReason::Unknown);
                }
            }
        } else {
            for (collision, reason) in colliding_transactions.iter() {
                if !self.is_better_than_collision(transaction, *collision, storage) {
                    if let Some(reason) = reason.first() {
                        return Err(reason.clone());
                    } else {
                        return Err(CollisionReason::Unknown);
                    }
                }
            }
        }
        Ok(())
    }

    fn on_stored_transaction(
        &mut self,
        transaction: &PoolTransaction,
        transaction_storage_id: S::StorageIndex,
    ) -> Result<(), Error> {
        if let PoolTransaction::Blob(checked_tx, _) = transaction {
            let blob_id = checked_tx.transaction().blob_id();
            self.blobs_users.insert(*blob_id, transaction_storage_id);
        }
        for input in transaction.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // insert coin
                    self.coins_spenders.insert(*utxo_id, transaction_storage_id);
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // insert message
                    self.messages_spenders
                        .insert(*nonce, transaction_storage_id);
                }
                _ => {}
            }
        }
        for output in transaction.outputs().iter() {
            match output {
                Output::Coin { .. }
                | Output::Change { .. }
                | Output::Variable { .. }
                | Output::Contract(_) => {}
                Output::ContractCreated { contract_id, .. } => {
                    // insert contract
                    self.contracts_creators
                        .insert(*contract_id, transaction_storage_id);
                }
            };
        }
        Ok(())
    }

    fn on_removed_transaction(
        &mut self,
        transaction: &PoolTransaction,
    ) -> Result<(), Error> {
        if let PoolTransaction::Blob(checked_tx, _) = transaction {
            let blob_id = checked_tx.transaction().blob_id();
            self.blobs_users.remove(blob_id);
        }
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
                _ => {}
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
        Ok(())
    }
}
