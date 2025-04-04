//! Temporary module to extract outputs until we know that they have been settled in database or skipped.

use std::collections::HashMap;

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
        TxId,
        UtxoId,
    },
    services::txpool::ArcPoolTx,
};

pub struct ExtractedOutputs {
    contract_created: HashMap<ContractId, TxId>,
    contract_created_by_tx: HashMap<TxId, Vec<ContractId>>,
    coins_created: HashMap<TxId, HashMap<u16, (Address, u64, AssetId)>>,
}

impl ExtractedOutputs {
    pub fn new() -> Self {
        Self {
            contract_created: HashMap::new(),
            contract_created_by_tx: HashMap::new(),
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
    pub fn new_extracted_outputs<'a>(
        &mut self,
        outputs: impl Iterator<Item = &'a (UtxoId, Output)>,
    ) {
        for (utxo_id, output) in outputs {
            match output {
                Output::ContractCreated { contract_id, .. } => {
                    self.contract_created.insert(*contract_id, *utxo_id.tx_id());
                }
                Output::Coin {
                    to,
                    amount,
                    asset_id,
                }
                | Output::Change {
                    amount,
                    asset_id,
                    to,
                }
                | Output::Variable {
                    amount,
                    asset_id,
                    to,
                } => {
                    self.coins_created
                        .entry(*utxo_id.tx_id())
                        .or_default()
                        .insert(utxo_id.output_index(), (*to, *amount, *asset_id));
                }
                Output::Contract { .. } => {
                    continue;
                }
            }
        }
    }

    pub fn new_extracted_transaction(&mut self, tx: &ArcPoolTx) {
        let tx_id = tx.id();
        for (idx, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::ContractCreated { contract_id, .. } => {
                    self.contract_created.insert(*contract_id, tx_id);
                    self.contract_created_by_tx
                        .entry(tx_id)
                        .or_default()
                        .push(*contract_id);
                }
                Output::Coin {
                    to,
                    amount,
                    asset_id,
                } => {
                    self.coins_created.entry(tx_id).or_default().insert(
                        u16::try_from(idx).expect("Outputs count is less than u16::MAX"),
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
                    self.coins_created
                        .entry(*utxo_id.tx_id())
                        .and_modify(|coins| {
                            coins.remove(&utxo_id.output_index());
                        });
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

    pub fn new_skipped_transaction(&mut self, tx_id: &TxId) {
        self.new_executed_transaction(tx_id);
    }

    pub fn new_executed_transaction(&mut self, tx_id: &TxId) {
        let contract_ids = self.contract_created_by_tx.remove(tx_id);
        if let Some(contract_ids) = contract_ids {
            for contract_id in contract_ids {
                self.contract_created.remove(&contract_id);
            }
        }
        self.coins_created.remove(tx_id);
    }

    pub fn contract_exists(&self, contract_id: &ContractId) -> bool {
        self.contract_created.contains_key(contract_id)
    }

    pub fn coin_exists(
        &self,
        utxo_id: &UtxoId,
        address: &Address,
        amount: &u64,
        asset_id: &AssetId,
    ) -> bool {
        self.coins_created
            .get(utxo_id.tx_id())
            .is_some_and(|coins| {
                coins
                    .get(&utxo_id.output_index())
                    .is_some_and(|(a, am, asid)| {
                        a == address && am == amount && asid == asset_id
                    })
            })
    }
}
