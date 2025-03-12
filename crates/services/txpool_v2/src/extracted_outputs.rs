//! Temporary module to extract outputs until we know that they have been settled in database or skipped.

use std::collections::{
    HashMap,
    HashSet,
};

use fuel_core_types::{
    fuel_tx::{
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
        Address,
        AssetId,
        ContractId,
        Input,
        Output,
        UtxoId,
    },
    services::txpool::ArcPoolTx,
};

pub struct ExtractedOutputs {
    contract_created: HashSet<ContractId>,
    coins_created: HashMap<UtxoId, (Address, u64, AssetId)>,
}

impl ExtractedOutputs {
    pub fn new() -> Self {
        Self {
            contract_created: HashSet::new(),
            coins_created: HashMap::new(),
        }
    }
}

impl Default for ExtractedOutputs {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractedOutputs {
    pub fn new_extracted_transaction(&mut self, tx: &ArcPoolTx) {
        for (idx, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::ContractCreated { contract_id, .. } => {
                    self.contract_created.insert(*contract_id);
                }
                Output::Coin {
                    to,
                    amount,
                    asset_id,
                } => {
                    self.coins_created.insert(
                        UtxoId::new(
                            tx.id(),
                            u16::try_from(idx)
                                .expect("Outputs count is less than u16::MAX"),
                        ),
                        (*to, *amount, *asset_id),
                    );
                }
                Output::Contract { .. }
                | Output::Change { .. }
                | Output::Variable { .. } => {
                    continue;
                }
            }
        }
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    self.coins_created.remove(utxo_id);
                }
                Input::Contract(_)
                | Input::MessageCoinPredicate(_)
                | Input::MessageCoinSigned(_)
                | Input::MessageDataPredicate(_)
                | Input::MessageDataSigned(_) => {
                    continue;
                }
            }
        }
    }

    pub fn new_executed_transaction(&mut self, tx: &ArcPoolTx) {
        for (idx, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::ContractCreated { contract_id, .. } => {
                    self.contract_created.remove(contract_id);
                }
                Output::Coin { .. } => {
                    self.coins_created.remove(&UtxoId::new(
                        tx.id(),
                        u16::try_from(idx).expect("Outputs count is less than u16::MAX"),
                    ));
                }
                Output::Contract { .. }
                | Output::Change { .. }
                | Output::Variable { .. } => {
                    continue;
                }
            }
        }
    }

    pub fn contract_exists(&self, contract_id: &ContractId) -> bool {
        self.contract_created.contains(contract_id)
    }

    pub fn coin_exists(
        &self,
        utxo_id: &UtxoId,
        address: &Address,
        amount: &u64,
        asset_id: &AssetId,
    ) -> bool {
        self.coins_created
            .get(utxo_id)
            .map_or(false, |(addr, amt, asset)| {
                addr == address && amt == amount && asset == asset_id
            })
    }
}
