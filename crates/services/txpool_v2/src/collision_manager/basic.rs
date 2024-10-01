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

use crate::{
    error::{
        CollisionReason,
        Error,
    },
    storage::StorageData,
};

use super::{
    CollisionManager,
    Collisions,
};

pub struct BasicCollisionManager<StorageIndex> {
    /// Message -> Transaction that currently use the Message
    messages_spenders: HashMap<Nonce, StorageIndex>,
    /// Coins -> Transaction that currently use the UTXO
    coins_spenders: HashMap<UtxoId, StorageIndex>,
    /// Contract -> Transaction that currently create the contract
    contracts_creators: HashMap<ContractId, StorageIndex>,
    /// Blob -> Transaction that currently create the blob
    blobs_users: HashMap<BlobId, StorageIndex>,
}

impl<StorageIndex> BasicCollisionManager<StorageIndex> {
    pub fn new() -> Self {
        Self {
            messages_spenders: HashMap::new(),
            coins_spenders: HashMap::new(),
            contracts_creators: HashMap::new(),
            blobs_users: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.messages_spenders.is_empty()
            && self.coins_spenders.is_empty()
            && self.contracts_creators.is_empty()
            && self.blobs_users.is_empty()
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
                        return Err(Error::DuplicateTxId(transaction.id()));
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
        if let PoolTransaction::Blob(checked_tx, _) = &store_entry.transaction {
            let blob_id = checked_tx.transaction().blob_id();
            self.blobs_users.insert(*blob_id, storage_id);
        }
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
                _ => {}
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
    }
}
