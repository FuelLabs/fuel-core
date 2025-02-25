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

use crate::error::{
    Error,
    InputValidationError,
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
pub(crate) struct PendingPool {
    ttl: Duration,
    unresolved_txs_by_inputs: HashMap<MissingInput, Vec<TxId>>,
    unresolved_inputs_by_tx: HashMap<TxId, (ArcPoolTx, Vec<MissingInput>)>,
    ttl_check: VecDeque<(SystemTime, TxId)>,
    pub(crate) current_bytes: usize,
    pub(crate) current_txs: usize,
    pub(crate) current_gas: u64,
}

#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
pub(crate) enum MissingInput {
    Utxo(UtxoId),
    Contract(ContractId),
}

impl From<MissingInput> for Error {
    fn from(value: MissingInput) -> Self {
        match value {
            MissingInput::Utxo(utxo) => {
                Error::InputValidation(InputValidationError::UtxoNotFound(utxo))
            }
            MissingInput::Contract(contract) => Error::InputValidation(
                InputValidationError::NotInsertedInputContractDoesNotExist(contract),
            ),
        }
    }
}

impl From<&MissingInput> for Error {
    fn from(value: &MissingInput) -> Self {
        match value {
            MissingInput::Utxo(utxo) => {
                Error::InputValidation(InputValidationError::UtxoNotFound(*utxo))
            }
            MissingInput::Contract(contract) => Error::InputValidation(
                InputValidationError::NotInsertedInputContractDoesNotExist(*contract),
            ),
        }
    }
}

pub(crate) struct UpdateMissingTxs {
    pub resolved_txs: Vec<ArcPoolTx>,
    pub expired_txs: Vec<ArcPoolTx>,
}

impl PendingPool {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            unresolved_txs_by_inputs: HashMap::default(),
            unresolved_inputs_by_tx: HashMap::default(),
            ttl_check: VecDeque::new(),
            current_bytes: 0,
            current_txs: 0,
            current_gas: 0,
        }
    }

    pub fn insert_transaction(
        &mut self,
        transaction: ArcPoolTx,
        missing_inputs: Vec<MissingInput>,
    ) {
        let tx_id = transaction.id();
        self.current_bytes = self
            .current_bytes
            .saturating_add(transaction.metered_bytes_size());
        self.current_gas = self.current_gas.saturating_add(transaction.max_gas());
        self.current_txs = self.current_txs.saturating_add(1);
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
            if *ttl < now {
                break;
            }
            if let Some((tx, missing_inputs)) = self.unresolved_inputs_by_tx.remove(tx_id)
            {
                self.current_bytes =
                    self.current_bytes.saturating_sub(tx.metered_bytes_size());
                self.current_gas = self.current_gas.saturating_sub(tx.max_gas());
                self.current_txs = self.current_txs.saturating_sub(1);
                updated_txs.expired_txs.push(tx);
                for input in missing_inputs {
                    // Remove tx_id from the list of unresolved transactions for this input
                    // If the list becomes empty, remove the entry
                    if let Some(tx_ids) = self.unresolved_txs_by_inputs.get_mut(&input) {
                        tx_ids.retain(|id| *id != *tx_id);
                        if tx_ids.is_empty() {
                            self.unresolved_txs_by_inputs.remove(&input);
                        }
                    }
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

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.unresolved_inputs_by_tx.is_empty()
            && self.unresolved_txs_by_inputs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fuel_core_types::{
        fuel_asm::{
            op,
            RegId,
        },
        fuel_tx::{
            Address,
            AssetId,
            ConsensusParameters,
            Finalizable,
            Input,
            Output,
            TransactionBuilder,
            UtxoId,
        },
        fuel_vm::{
            checked_transaction::{
                CheckPredicateParams,
                EstimatePredicates,
                IntoChecked,
            },
            interpreter::MemoryInstance,
            predicate::EmptyStorage,
        },
        services::txpool::{
            ArcPoolTx,
            Metadata,
            PoolTransaction,
        },
    };
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use super::*;

    fn create_pool_tx(
        inputs: Vec<UtxoId>,
        outputs: Vec<Output>,
        rng: &mut StdRng,
    ) -> ArcPoolTx {
        let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        let mut tx = TransactionBuilder::script(vec![], vec![]);
        tx.script_gas_limit(1000000);
        if inputs.is_empty() {
            tx.add_input(Input::coin_predicate(
                UtxoId::new([0; 32].into(), 0),
                owner,
                outputs
                    .iter()
                    .map(|output| match output {
                        Output::Coin { amount, .. } => *amount,
                        _ => 0,
                    })
                    .sum(),
                AssetId::default(),
                Default::default(),
                Default::default(),
                predicate.clone(),
                Default::default(),
            ));
        } else {
            for input in inputs {
                tx.add_input(Input::coin_predicate(
                    input,
                    owner,
                    10,
                    AssetId::default(),
                    Default::default(),
                    Default::default(),
                    predicate.clone(),
                    Default::default(),
                ));
            }
        }
        for output in outputs {
            tx.add_output(output);
        }
        // To differentiates the transactions
        tx.add_random_fee_input(rng);
        let mut tx = tx.finalize();
        tx.estimate_predicates(
            &CheckPredicateParams::default(),
            MemoryInstance::new(),
            &EmptyStorage,
        )
        .expect("Failed to estimate predicates");
        let tx = tx
            .into_checked_basic(1u32.into(), &ConsensusParameters::default())
            .unwrap();
        Arc::new(PoolTransaction::Script(tx, Metadata::new(1, 1, 0)))
    }

    #[test]
    fn test_pending_pool_one_tx_one_dependent_input() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let mut pending_pool = PendingPool::new(Duration::from_secs(1));

        // Given
        let dependency_tx = create_pool_tx(
            vec![],
            vec![Output::Coin {
                to: Address::default(),
                asset_id: AssetId::default(),
                amount: 100,
            }],
            &mut rng,
        );
        let utxo = UtxoId::new(dependency_tx.id(), 0);
        let dependent_tx = create_pool_tx(vec![utxo], vec![], &mut rng);

        // When
        pending_pool
            .insert_transaction(dependent_tx.clone(), vec![MissingInput::Utxo(utxo)]);
        let new_known_txs = vec![dependency_tx.clone()];
        let updated_txs = pending_pool.new_known_txs(new_known_txs);

        // Then
        assert_eq!(updated_txs.resolved_txs.len(), 1);
        assert_eq!(updated_txs.resolved_txs[0].id(), dependent_tx.id());
        assert!(updated_txs.expired_txs.is_empty());
        assert!(pending_pool.is_empty());
    }

    #[test]
    fn test_pending_pool_two_tx_one_dependent_input() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let mut pending_pool = PendingPool::new(Duration::from_secs(1));

        // Given
        let dependency_tx = create_pool_tx(
            vec![],
            vec![Output::Coin {
                to: Address::default(),
                asset_id: AssetId::default(),
                amount: 100,
            }],
            &mut rng,
        );
        let utxo = UtxoId::new(dependency_tx.id(), 0);
        let dependent_tx_1 = create_pool_tx(vec![utxo], vec![], &mut rng);
        let dependent_tx_2 = create_pool_tx(vec![utxo], vec![], &mut rng);

        // When
        pending_pool
            .insert_transaction(dependent_tx_1.clone(), vec![MissingInput::Utxo(utxo)]);
        pending_pool
            .insert_transaction(dependent_tx_2.clone(), vec![MissingInput::Utxo(utxo)]);
        let new_known_txs = vec![dependency_tx.clone()];
        let updated_txs = pending_pool.new_known_txs(new_known_txs);

        // Then
        assert_eq!(updated_txs.resolved_txs.len(), 2);
        assert!(updated_txs.expired_txs.is_empty());
        assert!(pending_pool.is_empty());
    }

    #[test]
    fn test_pending_pool_one_tx_two_dependent_inputs() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let mut pending_pool = PendingPool::new(Duration::from_secs(1));

        // Given
        let dependency_tx = create_pool_tx(
            vec![],
            vec![
                Output::Coin {
                    to: Address::default(),
                    asset_id: AssetId::default(),
                    amount: 100,
                },
                Output::Coin {
                    to: Address::default(),
                    asset_id: AssetId::default(),
                    amount: 100,
                },
            ],
            &mut rng,
        );
        let utxo_1 = UtxoId::new(dependency_tx.id(), 0);
        let utxo_2 = UtxoId::new(dependency_tx.id(), 1);
        let dependent_tx = create_pool_tx(vec![utxo_1, utxo_2], vec![], &mut rng);

        // When
        pending_pool.insert_transaction(
            dependent_tx.clone(),
            vec![MissingInput::Utxo(utxo_1), MissingInput::Utxo(utxo_2)],
        );
        let new_known_txs = vec![dependency_tx.clone()];
        let updated_txs = pending_pool.new_known_txs(new_known_txs);

        // Then
        assert_eq!(updated_txs.resolved_txs.len(), 1);
        assert_eq!(updated_txs.resolved_txs[0].id(), dependent_tx.id());
        assert!(updated_txs.expired_txs.is_empty());
        assert!(pending_pool.is_empty());
    }

    #[test]
    fn test_pending_pool_two_txs_expired() {
        let mut rng = StdRng::seed_from_u64(2322u64);

        // Given
        let mut pending_pool = PendingPool::new(Duration::from_millis(1));
        let tx1 = create_pool_tx(vec![], vec![], &mut rng);
        let tx2 = create_pool_tx(vec![], vec![], &mut rng);
        pending_pool.insert_transaction(
            tx1.clone(),
            vec![MissingInput::Utxo(UtxoId::new(tx1.id(), 0))],
        );
        pending_pool.insert_transaction(
            tx2.clone(),
            vec![MissingInput::Utxo(UtxoId::new(tx2.id(), 0))],
        );

        // When
        let updated_txs = pending_pool.new_known_txs(vec![]);

        // Then
        assert_eq!(updated_txs.resolved_txs.len(), 0);
        assert_eq!(updated_txs.expired_txs.len(), 2);
        assert!(pending_pool.is_empty());
    }
}
