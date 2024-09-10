use std::collections::HashMap;

use fuel_core_types::{
    fuel_tx::{
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
    storage::Storage,
};

use super::{
    CollisionManager,
    CollisionReason,
    Collisions,
};

pub struct BasicCollisionManager<S: Storage> {
    /// Message -> Transaction that currently use the Message
    messages_spenders: HashMap<Nonce, S::StorageIndex>,
    /// Coins -> Transaction that currently use the UTXO
    coins_spenders: HashMap<UtxoId, S::StorageIndex>,
    /// Contract -> Transaction that currenty create the contract
    contracts_creators: HashMap<ContractId, S::StorageIndex>,
}

impl<S: Storage> BasicCollisionManager<S> {
    pub fn new() -> Self {
        Self {
            messages_spenders: HashMap::new(),
            coins_spenders: HashMap::new(),
            contracts_creators: HashMap::new(),
        }
    }
}

impl<S: Storage> Default for BasicCollisionManager<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Storage> BasicCollisionManager<S> {
    fn gather_colliding_txs(
        &self,
        tx: &PoolTransaction,
    ) -> Result<Collisions<S::StorageIndex>, Error> {
        let mut collisions = Collisions::new();
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // Check if the utxo is already spent by another transaction in the pool
                    if let Some(tx_id) = self.coins_spenders.get(utxo_id) {
                        collisions.reasons.insert(CollisionReason::Coin(*utxo_id));
                        collisions.colliding_txs.push(*tx_id);
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // Check if the message is already spent by another transaction in the pool
                    if let Some(tx_id) = self.messages_spenders.get(nonce) {
                        collisions.reasons.insert(CollisionReason::Message(*nonce));
                        collisions.colliding_txs.push(*tx_id);
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
                    collisions
                        .reasons
                        .insert(CollisionReason::ContractCreation(*contract_id));
                    collisions.colliding_txs.push(*tx_id);
                }
            }
        }
        Ok(collisions)
    }

    fn is_better_than_collisions(
        &self,
        tx: &PoolTransaction,
        collisions: &Collisions<S::StorageIndex>,
        storage: &S,
    ) -> bool {
        let new_tx_ratio = Ratio::new(tx.tip(), tx.max_gas());
        let (total_tip, total_gas) = collisions.colliding_txs.iter().fold(
            (0u64, 0u64),
            |(total_tip, total_gas), node_id| {
                let dependent_tx = storage
                    .get(node_id)
                    .expect("Transaction always should exist in storage");
                let total_tip = total_tip.saturating_add(dependent_tx.cumulative_tip);
                let total_gas = total_gas.saturating_add(dependent_tx.cumulative_gas);
                (total_tip, total_gas)
            },
        );

        let collision_tx_ratio = Ratio::new(total_tip, total_gas);

        new_tx_ratio > collision_tx_ratio
    }
}

impl<S: Storage> CollisionManager<S> for BasicCollisionManager<S> {
    fn collect_tx_collisions(
        &self,
        transaction: &PoolTransaction,
        storage: &S,
    ) -> Result<Collisions<S::StorageIndex>, Error> {
        let collisions = self.gather_colliding_txs(transaction)?;
        if collisions.reasons.is_empty() {
            Ok(Collisions::new())
        } else if self.is_better_than_collisions(transaction, &collisions, storage) {
            Ok(collisions)
        } else {
            Err(Error::Collided(format!(
                "Transaction collides with other transactions: {:?}",
                collisions.colliding_txs
            )))
        }
    }

    fn on_stored_transaction(
        &mut self,
        transaction: &PoolTransaction,
        transaction_storage_id: S::StorageIndex,
    ) -> Result<(), Error> {
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
}
