use std::collections::{
    hash_map::Entry,
    HashMap,
    HashSet,
};

use fuel_core_types::{
    fuel_tx::{
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            contract::Contract,
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
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};
use petgraph::graph::NodeIndex;

use crate::{
    error::Error,
    pool::{
        CollisionReason,
        Collisions,
        Parents,
        TxGraph,
    },
    ports::TxPoolDb,
};

// TODO: change both to be nodeindex
type Spender = TxId;
type Creator = TxId;

pub struct Registries {
    /// Coins -> Transaction that crurrently create the UTXO
    pub coins_creators: HashMap<UtxoId, Creator>,
    /// Coins -> Transaction that currently use the UTXO
    pub coins_spenders: HashMap<UtxoId, Spender>,
    /// Contract -> Transaction that currenty create the contract
    pub contracts_creators: HashMap<ContractId, Creator>,
    /// Blob -> Transaction that currently create the blob
    pub blobs_creators: HashMap<BlobId, Creator>,
    /// Message -> Transaction that currently use the Message
    pub messages_spenders: HashMap<Nonce, Spender>,
}

impl Default for Registries {
    fn default() -> Self {
        Self::new()
    }
}

impl Registries {
    pub fn new() -> Self {
        Registries {
            coins_creators: HashMap::default(),
            coins_spenders: HashMap::default(),
            contracts_creators: HashMap::default(),
            blobs_creators: HashMap::default(),
            messages_spenders: HashMap::default(),
        }
    }

    // Gather all transactions that collide with the given transaction
    // TODO: Move to collision management trait that depends on storage
    pub fn gather_colliding_txs(
        &self,
        tx: &PoolTransaction,
    ) -> Result<Collisions, Error> {
        let mut collisions = HashMap::default();
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // Check if the utxo is already spent by another transaction in the pool
                    if let Some(tx_id) = self.coins_spenders.get(utxo_id) {
                        collisions.insert(CollisionReason::Coin(*utxo_id), *tx_id);
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // Check if the message is already spent by another transaction in the pool
                    if let Some(tx_id) = self.messages_spenders.get(nonce) {
                        collisions.insert(CollisionReason::Message(*nonce), *tx_id);
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
                        .insert(CollisionReason::ContractCreation(*contract_id), *tx_id);
                }
            }
        }
        Ok(collisions)
    }

    // Gather all transactions that are parents of the given transaction in the transaction graph
    // If the parent is neither in the graph nor in the database, error is returned
    pub fn check_and_gather_parent_txs(
        &self,
        tx: &PoolTransaction,
        collisions: Collisions,
        graph: &TxGraph,
        tx_to_node: &HashMap<TxId, NodeIndex>,
        db: &impl TxPoolDb,
        utxo_validation: bool,
    ) -> Result<Parents, Error> {
        //TODO: Finish the function
        let mut pool_parents = Vec::new();
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // If the utxo collides it means we already made the verifications
                    // If the utxo is created in the pool, need to check if we don't spend too much (utxo can still be unresolved)
                    // If the utxo_validation is active, we need to check if the utxo exists in the database and is valid
                    if collisions.get(&CollisionReason::Coin(*utxo_id)).is_some() {
                        continue;
                    }
                    if let Some(tx_id) = self.coins_creators.get(utxo_id) {
                        let output_tx =
                            graph
                                .node_weight(*tx_to_node.get(tx_id).expect(
                                    "Transaction always should exist in `tx_info`",
                                ))
                                .expect("Transaction always should exist in `tx_info`");
                        // TODO: Other checks
                        pool_parents.push(*tx_id);
                    }
                    if utxo_validation {
                        let Some(coin) = db
                            .utxo(utxo_id)
                            .map_err(|e| Error::Database(format!("{:?}", e)))?
                        else {
                            return Err(Error::UtxoNotFound(*utxo_id));
                        };
                        if !coin.matches_input(input).expect("The input is coin above") {
                            return Err(Error::NotInsertedIoCoinMismatch)
                        }
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                }
                _ => {}
            }
        }
        Ok(pool_parents)
    }

    pub fn insert_outputs_in_registry(
        &mut self,
        tx: &PoolTransaction,
    ) -> Result<(), Error> {
        for (index, output) in tx.outputs().iter().enumerate() {
            let index = u16::try_from(index).map_err(|_| {
                Error::WrongOutputNumber(format!(
                    "The number of outputs in `{}` is more than `u16::max`",
                    tx.id()
                ))
            })?;
            let tx_id = tx.id();
            match output {
                Output::Coin { .. } | Output::Change { .. } | Output::Variable { .. } => {
                    let utxo_id = UtxoId::new(tx.id(), index);
                    self.coins_creators.insert(utxo_id, tx_id);
                }
                Output::ContractCreated { contract_id, .. } => {
                    // insert contract
                    self.contracts_creators.insert(*contract_id, tx_id);
                }
                Output::Contract(_) => {
                    // do nothing, this contract is already found in dependencies.
                    // as it is tied with input and used_by is already inserted.
                }
            };
        }
        Ok(())
    }
}
