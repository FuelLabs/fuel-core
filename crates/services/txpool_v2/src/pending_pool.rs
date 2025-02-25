#![allow(dead_code)]
use std::{
    collections::{
        HashMap,
        VecDeque,
    },
    time::{
        Duration,
        SystemTime,
    },
};

use fuel_core_types::{
    fuel_tx::{
        ContractId,
        Output,
        TxId,
        UtxoId,
    },
    services::txpool::ArcPoolTx,
};

// This is a simple temporary storage for transactions that doesn't have all of their input created yet.
// This storage should not have a lot of complexity.
//
// Insertion rules:
// - If the transaction has one or more inputs that are not known yet.
//
// Deletion rules:
// - If the transaction missing inputs hasn't been known for a certain amount of time.
// - If the transaction missing inputs becomes known.
pub struct PendingPool {
    ttl: Duration,
    unresolved_txs_by_inputs: HashMap<MissingInput, Vec<TxId>>,
    unresolved_inputs_by_tx: HashMap<TxId, (ArcPoolTx, Vec<MissingInput>)>,
    ttl_check: VecDeque<(SystemTime, TxId)>,
}

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
pub enum MissingInput {
    Utxo(UtxoId),
    Contract(ContractId),
}

pub struct UpdateMissingTxs {
    pub resolved_txs: Vec<ArcPoolTx>,
    pub expired_txs: Vec<ArcPoolTx>,
}

impl PendingPool {
    // Review question: Should we add size ?
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            unresolved_txs_by_inputs: HashMap::default(),
            unresolved_inputs_by_tx: HashMap::default(),
            ttl_check: VecDeque::new(),
        }
    }

    pub fn insert_transaction(
        &mut self,
        transaction: ArcPoolTx,
        missing_inputs: Vec<MissingInput>,
    ) {
        let tx_id = transaction.id();
        for input in &missing_inputs {
            self.unresolved_txs_by_inputs
                .entry(*input)
                .or_default()
                .push(tx_id);
        }
        self.unresolved_inputs_by_tx
            .insert(transaction.id(), (transaction, missing_inputs));
        self.ttl_check.push_front((
            SystemTime::now()
                .checked_add(self.ttl)
                .expect("The system time should be valid; qed"),
            tx_id,
        ));
    }

    fn new_known_utxo(&mut self, utxo_id: UtxoId, resolved_txs: &mut Vec<ArcPoolTx>) {
        if let Some(tx_ids) = self
            .unresolved_txs_by_inputs
            .remove(&MissingInput::Utxo(utxo_id))
        {
            for tx_id in tx_ids {
                if let Some((tx, missing_inputs)) =
                    self.unresolved_inputs_by_tx.remove(&tx_id)
                {
                    let missing_inputs: Vec<MissingInput> = missing_inputs
                        .into_iter()
                        .filter(|input| match input {
                            MissingInput::Utxo(utxo) => *utxo != utxo_id,
                            _ => true,
                        })
                        .collect();
                    if missing_inputs.is_empty() {
                        resolved_txs.push(tx);
                    } else {
                        self.unresolved_inputs_by_tx
                            .insert(tx_id, (tx, missing_inputs));
                    }
                }
            }
        }
    }

    fn new_known_contract(
        &mut self,
        contract_id: ContractId,
        resolved_txs: &mut Vec<ArcPoolTx>,
    ) {
        if let Some(tx_ids) = self
            .unresolved_txs_by_inputs
            .remove(&MissingInput::Contract(contract_id))
        {
            for tx_id in tx_ids {
                if let Some((tx, missing_inputs)) =
                    self.unresolved_inputs_by_tx.remove(&tx_id)
                {
                    let missing_inputs: Vec<MissingInput> = missing_inputs
                        .into_iter()
                        .filter(|input| match input {
                            MissingInput::Contract(contract) => *contract != contract_id,
                            _ => true,
                        })
                        .collect();
                    if missing_inputs.is_empty() {
                        resolved_txs.push(tx);
                    } else {
                        self.unresolved_inputs_by_tx
                            .insert(tx_id, (tx, missing_inputs));
                    }
                }
            }
        }
    }

    fn expire_transactions(&mut self, updated_txs: &mut UpdateMissingTxs) {
        let now = SystemTime::now();
        while let Some((ttl, tx_id)) = self.ttl_check.back() {
            if *ttl > now {
                break;
            }
            if let Some((tx, missing_inputs)) = self.unresolved_inputs_by_tx.remove(tx_id)
            {
                updated_txs.expired_txs.push(tx);
                for input in missing_inputs {
                    if let Some(tx_ids) = self.unresolved_txs_by_inputs
                        .get_mut(&input) { tx_ids.retain(|id| *id != *tx_id) }
                }
            }
            self.ttl_check.pop_back();
        }
    }

    // We expect it to be called a lot (every new block, every new tx in the pool).
    pub fn new_known_txs(&mut self, new_known_txs: Vec<ArcPoolTx>) -> UpdateMissingTxs {
        let mut updated_txs = UpdateMissingTxs {
            resolved_txs: Vec::new(),
            expired_txs: Vec::new(),
        };
        // Resolve transactions
        for tx in new_known_txs {
            let tx_id = tx.id();
            for (index, output) in tx.outputs().iter().enumerate() {
                // SAFETY: We deal with CheckedTransaction there which should already check this
                let index = u16::try_from(index).expect(
                    "The number of outputs in a transaction should be less than `u16::max`",
                );
                let utxo_id = UtxoId::new(tx_id, index);
                match output {
                    Output::Coin { .. } => {
                        self.new_known_utxo(utxo_id, &mut updated_txs.resolved_txs);
                    }
                    Output::ContractCreated { contract_id, .. } => {
                        self.new_known_contract(
                            *contract_id,
                            &mut updated_txs.resolved_txs,
                        );
                    }
                    _ => {}
                }
            }
        }

        self.expire_transactions(&mut updated_txs);
        updated_txs
    }
}
