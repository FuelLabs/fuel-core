use std::{
    collections::HashMap,
    fmt::Debug,
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
    error::Error,
    storage::StorageData,
};

use super::CollisionManager;

pub trait BasicCollisionManagerStorage {
    type StorageIndex: Copy + Debug + PartialEq + Eq;

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
    fn gather_colliding_tx(
        &self,
        tx: &PoolTransaction,
    ) -> Result<Option<S::StorageIndex>, Error> {
        let mut collision: Option<S::StorageIndex> = None;
        if let PoolTransaction::Blob(checked_tx, _) = tx {
            let blob_id = checked_tx.transaction().blob_id();
            if let Some(state) = self.blobs_users.get(blob_id) {
                if let Some(collision) = collision {
                    if state != &collision {
                        return Err(Error::Collided(format!(
                            "Transaction collides with other transactions: {:?}",
                            collision
                        )));
                    }
                }
                collision = Some(*state);
            }
        }
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // Check if the utxo is already spent by another transaction in the pool
                    if let Some(tx_id) = self.coins_spenders.get(utxo_id) {
                        if let Some(collision) = collision {
                            if tx_id != &collision {
                                return Err(Error::Collided(format!(
                                    "Transaction collides with other transactions: {:?}",
                                    collision
                                )));
                            }
                        }
                        collision = Some(*tx_id);
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // Check if the message is already spent by another transaction in the pool
                    if let Some(tx_id) = self.messages_spenders.get(nonce) {
                        if let Some(collision) = collision {
                            if tx_id != &collision {
                                return Err(Error::Collided(format!(
                                    "Transaction collides with other transactions: {:?}",
                                    collision
                                )));
                            }
                        }
                        collision = Some(*tx_id);
                    }
                }
                // No collision for contract inputs
                _ => {}
            }
        }

        for output in tx.outputs() {
            if let Output::ContractCreated { contract_id, .. } = output {
                // Check if the contract is already created by another transaction in the pool
                if let Some(tx_id) = self.contracts_creators.get(contract_id) {
                    if let Some(collision) = collision {
                        if tx_id != &collision {
                            return Err(Error::Collided(format!(
                                "Transaction collides with other transactions: {:?}",
                                collision
                            )));
                        }
                    }
                    collision = Some(*tx_id);
                }
            }
        }
        Ok(collision)
    }

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

    fn collect_colliding_transaction(
        &self,
        transaction: &PoolTransaction,
        storage: &S,
    ) -> Result<Option<S::StorageIndex>, Error> {
        let collision = self.gather_colliding_tx(transaction)?;
        if let Some(collision) = collision {
            if self.is_better_than_collision(transaction, collision, storage) {
                Ok(Some(collision))
            } else {
                Err(Error::Collided(format!(
                    "Transaction collides with other transactions: {:?}",
                    collision
                )))
            }
        } else {
            Ok(None)
        }
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
