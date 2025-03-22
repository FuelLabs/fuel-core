use std::{
    collections::{
        hash_map::Entry,
        HashMap,
        HashSet,
        VecDeque,
    },
    time::{
        Duration,
        SystemTime,
    },
};

use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_tx::{
        ContractId,
        Output,
        Transaction,
        TxId,
        UtxoId,
    },
    services::txpool::{
        utxo_ids_with_outputs,
        ArcPoolTx,
    },
};
use tokio::sync::mpsc::Sender;

use crate::{
    error::{
        Error,
        InputValidationError,
    },
    pool_worker::{
        InsertionSource,
        PoolNotification,
    },
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
    pending_txs_by_inputs: HashMap<MissingInput, HashSet<TxId>>,
    pending_inputs_by_tx: HashMap<TxId, PendingTx>,
    ttl_check: VecDeque<(SystemTime, TxId)>,
    pub(crate) current_bytes: usize,
    pub(crate) current_txs: usize,
    pub(crate) current_gas: u64,
}

#[derive(Debug)]
pub(crate) struct PendingTx {
    pub tx: ArcPoolTx,
    pub insertion_source: InsertionSource,
    pub missing_inputs: Vec<MissingInput>,
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

impl PendingPool {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            pending_txs_by_inputs: HashMap::default(),
            pending_inputs_by_tx: HashMap::default(),
            ttl_check: VecDeque::new(),
            current_bytes: 0,
            current_txs: 0,
            current_gas: 0,
        }
    }

    pub fn insert_transaction(
        &mut self,
        transaction: ArcPoolTx,
        insertion_source: InsertionSource,
        missing_inputs: Vec<MissingInput>,
    ) {
        let tx_id = transaction.id();
        self.current_bytes = self
            .current_bytes
            .saturating_add(transaction.metered_bytes_size());
        self.current_gas = self.current_gas.saturating_add(transaction.max_gas());
        self.current_txs = self.current_txs.saturating_add(1);
        for input in &missing_inputs {
            self.pending_txs_by_inputs
                .entry(*input)
                .or_default()
                .insert(tx_id);
        }
        self.pending_inputs_by_tx.insert(
            tx_id,
            PendingTx {
                tx: transaction,
                insertion_source,
                missing_inputs,
            },
        );
        self.ttl_check.push_front((
            SystemTime::now()
                .checked_add(self.ttl)
                .expect("The system time should be valid; qed"),
            tx_id,
        ));
    }

    fn new_known_input_from_output(
        &mut self,
        utxo_id: UtxoId,
        output: &Output,
        resolved_txs: &mut Vec<(ArcPoolTx, InsertionSource)>,
    ) {
        let missing_input = match output {
            Output::Coin { .. } => MissingInput::Utxo(utxo_id),
            Output::ContractCreated { contract_id, .. } => {
                MissingInput::Contract(*contract_id)
            }
            Output::Contract { .. } | Output::Change { .. } | Output::Variable { .. } => {
                return;
            }
        };

        if let Entry::Occupied(entry) = self.pending_txs_by_inputs.entry(missing_input) {
            for tx_id in entry.remove() {
                if let Entry::Occupied(mut entry) = self.pending_inputs_by_tx.entry(tx_id)
                {
                    let pending_tx = entry.get_mut();
                    pending_tx
                        .missing_inputs
                        .retain(|input| *input != missing_input);
                    if pending_tx.missing_inputs.is_empty() {
                        let PendingTx {
                            tx,
                            insertion_source,
                            missing_inputs: _,
                        } = entry.remove();
                        self.decrease_pool_size(&tx);
                        resolved_txs.push((tx, insertion_source));
                    }
                }
            }
        }
    }

    pub fn expire_transactions(&mut self, notification_sender: Sender<PoolNotification>) {
        let now = SystemTime::now();
        while let Some((ttl, tx_id)) = self.ttl_check.back().copied() {
            if ttl > now {
                break;
            }
            if let Some(PendingTx {
                tx,
                insertion_source,
                missing_inputs,
            }) = self.pending_inputs_by_tx.remove(&tx_id)
            {
                self.decrease_pool_size(&tx);
                for input in &missing_inputs {
                    // Remove tx_id from the list of unresolved transactions for this input
                    // If the list becomes empty, remove the entry
                    if let Entry::Occupied(mut tx_ids) =
                        self.pending_txs_by_inputs.entry(*input)
                    {
                        tx_ids.get_mut().remove(&tx_id);
                        if tx_ids.get().is_empty() {
                            tx_ids.remove();
                        }
                    }
                }
                if let Some(missing_input) = missing_inputs.first() {
                    if let Err(e) =
                        notification_sender.try_send(PoolNotification::ErrorInsertion {
                            tx_id: tx.id(),
                            source: insertion_source,
                            error: missing_input.into(),
                        })
                    {
                        tracing::error!(
                            "Failed to send error insertion notification: {}",
                            e
                        );
                    }
                }
            }
            self.ttl_check.pop_back();
        }
    }

    fn decrease_pool_size(&mut self, tx: &ArcPoolTx) {
        self.current_bytes = self.current_bytes.saturating_sub(tx.metered_bytes_size());
        self.current_gas = self.current_gas.saturating_sub(tx.max_gas());
        self.current_txs = self.current_txs.saturating_sub(1);
    }

    // We expect it to be called a lot (every new tx in the pool).
    pub fn new_known_tx<'a>(
        &mut self,
        new_known_tx_outputs: impl Iterator<Item = (UtxoId, &'a Output)>,
    ) -> Vec<(ArcPoolTx, InsertionSource)> {
        let mut res = Vec::new();
        self.new_known_tx_inner(new_known_tx_outputs, &mut res);
        res
    }

    // We expect it to be called a lot (every new block).
    pub fn new_known_txs<'a>(
        &mut self,
        new_known_txs: impl Iterator<Item = (&'a Transaction, TxId)>,
    ) -> Vec<(ArcPoolTx, InsertionSource)> {
        let mut res = Vec::new();
        // Resolve transactions
        for (tx, tx_id) in new_known_txs {
            let outputs = tx.outputs();
            let iter = utxo_ids_with_outputs(outputs.iter(), tx_id);
            self.new_known_tx_inner(iter, &mut res);
        }
        res
    }

    fn new_known_tx_inner<'a>(
        &mut self,
        new_known_outputs: impl Iterator<Item = (UtxoId, &'a Output)>,
        resolved_txs: &mut Vec<(ArcPoolTx, InsertionSource)>,
    ) {
        for (utxo_id, output) in new_known_outputs {
            self.new_known_input_from_output(utxo_id, output, resolved_txs);
        }
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.pending_inputs_by_tx.is_empty() && self.pending_txs_by_inputs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use std::sync::Arc;

    use fuel_core_types::{
        fuel_asm::{
            op,
            RegId,
        },
        fuel_tx::{
            output::contract::Contract as OutputContract,
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
            Contract,
        },
        services::txpool::{
            ArcPoolTx,
            Metadata,
            PoolTransaction,
        },
    };
    use futures::FutureExt;
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use super::*;

    fn create_tx(
        inputs: Vec<MissingInput>,
        outputs: Vec<Output>,
        rng: &mut StdRng,
    ) -> Transaction {
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
            for (idx, input) in inputs.iter().enumerate() {
                match input {
                    MissingInput::Utxo(input) => {
                        tx.add_input(Input::coin_predicate(
                            *input,
                            owner,
                            10,
                            AssetId::default(),
                            Default::default(),
                            Default::default(),
                            predicate.clone(),
                            Default::default(),
                        ));
                    }
                    MissingInput::Contract(contract_id) => {
                        tx.add_input(Input::contract(
                            UtxoId::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            *contract_id,
                        ));

                        tx.add_output(Output::Contract(OutputContract {
                            input_index: idx.try_into().unwrap(),
                            balance_root: Default::default(),
                            state_root: Default::default(),
                        }));
                    }
                }
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
        tx.into()
    }

    fn create_pool_tx(
        inputs: Vec<MissingInput>,
        outputs: Vec<Output>,
        rng: &mut StdRng,
    ) -> ArcPoolTx {
        let tx = create_tx(inputs, outputs, rng);
        let tx = tx.as_script().unwrap().clone();
        let tx = tx
            .into_checked_basic(1u32.into(), &ConsensusParameters::default())
            .unwrap();
        Arc::new(PoolTransaction::Script(tx, Metadata::new(1, 1, 0)))
    }

    #[test]
    fn new_known_tx__returns_expected_tx_when_one_dependent_input_provided() {
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
        let dependent_tx =
            create_pool_tx(vec![MissingInput::Utxo(utxo)], vec![], &mut rng);

        // When
        pending_pool.insert_transaction(
            dependent_tx.clone(),
            InsertionSource::RPC {
                response_channel: None,
            },
            vec![MissingInput::Utxo(utxo)],
        );
        let resolved_txs =
            pending_pool.new_known_tx(dependency_tx.utxo_ids_with_outputs());
        // Then
        assert_eq!(resolved_txs.len(), 1);
        assert_eq!(resolved_txs[0].0.id(), dependent_tx.id());
        assert!(pending_pool.is_empty());
    }

    #[test]
    fn new_known_tx__returns_expected_tx_when_one_dependent_input_contract_provided() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let mut pending_pool = PendingPool::new(Duration::from_secs(1));

        // Given
        let bytecode = op::ret(RegId::ONE).to_bytes().to_vec();
        let contract = Contract::from(bytecode.clone());
        let contract_root = contract.root();
        let state_root = Contract::default_state_root();
        let contract_id = contract.id(&Default::default(), &contract_root, &state_root);
        let mut dependency_tx = TransactionBuilder::create(
            bytecode.into(),
            Default::default(),
            Default::default(),
        );
        dependency_tx.add_output(Output::ContractCreated {
            contract_id,
            state_root: Default::default(),
        });
        dependency_tx.add_random_fee_input(&mut rng);
        let mut tx = dependency_tx.finalize();
        tx.estimate_predicates(
            &CheckPredicateParams::default(),
            MemoryInstance::new(),
            &EmptyStorage,
        )
        .expect("Failed to estimate predicates");
        let tx = tx
            .into_checked_basic(1u32.into(), &ConsensusParameters::default())
            .unwrap();
        let dependency_tx = Arc::new(PoolTransaction::Create(tx, Metadata::new(1, 1, 0)));
        let dependent_tx =
            create_pool_tx(vec![MissingInput::Contract(contract_id)], vec![], &mut rng);

        // When
        pending_pool.insert_transaction(
            dependent_tx.clone(),
            InsertionSource::RPC {
                response_channel: None,
            },
            vec![MissingInput::Contract(contract_id)],
        );
        let resolved_txs =
            pending_pool.new_known_tx(dependency_tx.utxo_ids_with_outputs());
        // Then
        assert_eq!(resolved_txs.len(), 1);
        assert_eq!(resolved_txs[0].0.id(), dependent_tx.id());
        assert!(pending_pool.is_empty());
    }

    #[test]
    fn new_known_tx__returns_expected_two_txs_when_common_dependent_input_provided() {
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
        let dependent_tx_1 =
            create_pool_tx(vec![MissingInput::Utxo(utxo)], vec![], &mut rng);
        let dependent_tx_2 =
            create_pool_tx(vec![MissingInput::Utxo(utxo)], vec![], &mut rng);

        // When
        pending_pool.insert_transaction(
            dependent_tx_1.clone(),
            InsertionSource::RPC {
                response_channel: None,
            },
            vec![MissingInput::Utxo(utxo)],
        );
        pending_pool.insert_transaction(
            dependent_tx_2.clone(),
            InsertionSource::RPC {
                response_channel: None,
            },
            vec![MissingInput::Utxo(utxo)],
        );
        let resolved_txs =
            pending_pool.new_known_tx(dependency_tx.utxo_ids_with_outputs());
        // Then
        assert_eq!(resolved_txs.len(), 2);
        assert!(pending_pool.is_empty());
    }

    #[test]
    fn new_known_tx__returns_expected_one_tx_when_two_dependent_input_provided() {
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
        let dependent_tx = create_pool_tx(
            vec![MissingInput::Utxo(utxo_1), MissingInput::Utxo(utxo_2)],
            vec![],
            &mut rng,
        );

        // When
        pending_pool.insert_transaction(
            dependent_tx.clone(),
            InsertionSource::RPC {
                response_channel: None,
            },
            vec![MissingInput::Utxo(utxo_1), MissingInput::Utxo(utxo_2)],
        );
        let resolved_txs =
            pending_pool.new_known_tx(dependency_tx.utxo_ids_with_outputs());
        // Then
        assert_eq!(resolved_txs.len(), 1);
        assert_eq!(resolved_txs[0].0.id(), dependent_tx.id());
        assert!(pending_pool.is_empty());
    }

    #[test]
    fn new_known_tx__returns_expired_txs_when_ttl_reached() {
        let mut rng = StdRng::seed_from_u64(2322u64);

        // Given
        let mut pending_pool = PendingPool::new(Duration::from_nanos(1));
        let tx1 = create_pool_tx(vec![], vec![], &mut rng);
        let tx2 = create_pool_tx(vec![], vec![], &mut rng);
        pending_pool.insert_transaction(
            tx1.clone(),
            InsertionSource::RPC {
                response_channel: None,
            },
            vec![MissingInput::Utxo(UtxoId::new(tx1.id(), 0))],
        );
        pending_pool.insert_transaction(
            tx2.clone(),
            InsertionSource::RPC {
                response_channel: None,
            },
            vec![MissingInput::Utxo(UtxoId::new(tx2.id(), 0))],
        );

        // When
        let resolved_txs = pending_pool.new_known_txs(vec![].into_iter());
        let (tx, mut rx) = tokio::sync::mpsc::channel(20);
        pending_pool.expire_transactions(tx);

        // Then
        let mut buf = Vec::new();
        assert_eq!(resolved_txs.len(), 0);
        rx.recv_many(&mut buf, 2).now_or_never().unwrap();
        assert_eq!(buf.len(), 2);
        assert!(pending_pool.is_empty());
    }
}
