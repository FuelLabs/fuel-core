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
    unresolved_txs: HashMap<MissingInput, ArcPoolTx>,
    ttl_check: VecDeque<(SystemTime, MissingInput)>,
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
            unresolved_txs: HashMap::default(),
            ttl_check: VecDeque::new(),
        }
    }

    // Review question: When we are missing an input in `validate_inputs` we return directly an error.
    // It means that it can be possible that we are waiting for 2 UTXOs and we will only get one.
    // Should we change the behavior of `validate_inputs` to return all the missing inputs ?
    //
    // Review question: Should we add collision resolution if there is already a transaction with the same missing input ?
    // I think yes
    pub fn insert_transaction(
        &mut self,
        transaction: ArcPoolTx,
        missing_input: MissingInput,
    ) {
        self.unresolved_txs.insert(missing_input, transaction);
        self.ttl_check
            .push_front((SystemTime::now() + self.ttl, missing_input));
    }

    // Review question: Is it ok to check ttl of pending transactions in this function ?
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
                    // Review question: Should we check `Variable` and `Change` outputs ? Or they
                    // are resolved to `Coin` after execution of transaction ?
                    Output::Coin { .. } => {
                        if let Some(tx) =
                            self.unresolved_txs.remove(&MissingInput::Utxo(utxo_id))
                        {
                            updated_txs.resolved_txs.push(tx);
                        }
                    }
                    Output::ContractCreated { contract_id, .. } => {
                        if let Some(tx) = self
                            .unresolved_txs
                            .remove(&MissingInput::Contract(*contract_id))
                        {
                            updated_txs.resolved_txs.push(tx);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Expire transactions
        let now = SystemTime::now();

        while let Some((ttl, missing_input)) = self.ttl_check.back() {
            if *ttl > now {
                break;
            }
            if let Some(tx) = self.unresolved_txs.remove(missing_input) {
                updated_txs.expired_txs.push(tx);
            }
            self.ttl_check.pop_back();
        }
        updated_txs
    }
}
