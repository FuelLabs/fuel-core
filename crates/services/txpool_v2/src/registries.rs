use std::collections::{
    hash_map::Entry,
    HashMap,
    HashSet,
};

use fuel_core_types::{
    fuel_tx::{
        BlobId,
        ContractId,
        Output,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};

use crate::error::Error;

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

    pub fn extend(&mut self, registries: Registries) {
        self.coins_creators
            .extend(registries.coins_creators.into_iter());
        self.coins_spenders
            .extend(registries.coins_spenders.into_iter());
        self.contracts_creators
            .extend(registries.contracts_creators.into_iter());
        self.blobs_creators
            .extend(registries.blobs_creators.into_iter());
        self.messages_spenders
            .extend(registries.messages_spenders.into_iter());
    }
}
