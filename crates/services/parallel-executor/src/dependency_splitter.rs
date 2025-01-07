use fuel_core_types::{
    fuel_tx::{
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            message,
        },
        output,
        ConsensusParameters,
        ContractId,
        Input,
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    fuel_vm::checked_transaction::CheckedTransaction,
    services::executor::{
        Error as ExecutorError,
        Result as ExecutorResult,
        TransactionValidityError,
    },
};
use fuel_core_upgradable_executor::native_executor::ports::{
    MaybeCheckedTransaction,
    TransactionExt,
};
use rustc_hash::{
    FxHashMap,
    FxHashSet,
};
use std::{
    cmp::Reverse,
    collections::BTreeSet,
    num::NonZeroUsize,
};

type Gas = u64;

#[derive(Default)]
struct OrderedBucket {
    gas: Gas,
    elements: Vec<TxId>,
}

impl OrderedBucket {
    fn new() -> Self {
        Self {
            elements: Vec::new(),
            ..Default::default()
        }
    }
}

impl OrderedBucket {
    fn add(&mut self, tx_id: TxId, gas: u64) {
        self.gas += gas;
        self.elements.push(tx_id);
    }
}

#[derive(Default)]
pub struct UnorderedBucket {
    gas: Gas,
    // Gas used to be used when passed to an other bucket and to order transactions afterwards
    elements: FxHashMap<TxId, Gas>,
}

impl UnorderedBucket {
    fn new(txs_len: usize) -> Self {
        Self {
            elements: FxHashMap::with_capacity_and_hasher(txs_len, Default::default()),
            ..Default::default()
        }
    }
}

impl UnorderedBucket {
    fn add(&mut self, tx_id: TxId, gas: u64) {
        self.gas += gas;
        self.elements.insert(tx_id, gas);
    }

    fn remove(&mut self, tx_id: &TxId) -> Option<u64> {
        self.elements.remove(tx_id)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum BucketIndex {
    Independent,
    Other,
}

pub struct DependencySplitter {
    independent_bucket: UnorderedBucket,
    other_buckets: OrderedBucket,
    blobs_bucket: OrderedBucket,
    txs_to_bucket: FxHashMap<TxId, (MaybeCheckedTransaction, BucketIndex)>,
    used_coins: FxHashSet<UtxoId>,
    used_messages: FxHashSet<Nonce>,
    created_contracts: FxHashMap<ContractId, TxId>,
    consensus_parameters: ConsensusParameters,
    remaining_block_gas: u64,
}

impl DependencySplitter {
    pub fn new(consensus_parameters: ConsensusParameters, txs_len: usize) -> Self {
        Self {
            independent_bucket: UnorderedBucket::new(txs_len),
            other_buckets: OrderedBucket::new(),
            blobs_bucket: OrderedBucket::new(),
            txs_to_bucket: FxHashMap::with_capacity_and_hasher(
                txs_len,
                Default::default(),
            ),
            used_coins: FxHashSet::with_capacity_and_hasher(txs_len, Default::default()),
            used_messages: FxHashSet::with_capacity_and_hasher(
                txs_len,
                Default::default(),
            ),
            created_contracts: FxHashMap::with_capacity_and_hasher(
                txs_len,
                Default::default(),
            ),
            remaining_block_gas: consensus_parameters.block_gas_limit(),
            consensus_parameters,
        }
    }

    pub fn process(
        &mut self,
        tx: MaybeCheckedTransaction,
        tx_id: TxId,
    ) -> ExecutorResult<()> {
        let gas = tx.max_gas(&self.consensus_parameters)?;
        let remaining_block_gas = self.remaining_block_gas;
        self.remaining_block_gas = self.remaining_block_gas.checked_sub(gas).ok_or(
            ExecutorError::GasOverflow(
                format!(
                "Transaction cannot fit in remaining gas limit: ({remaining_block_gas})."
            ),
                gas,
                remaining_block_gas,
            ),
        )?;

        let inputs = tx.inputs()?;

        if self.txs_to_bucket.contains_key(&tx_id) {
            return Err(ExecutorError::TransactionIdCollision(tx_id))
        }

        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    if self.used_coins.contains(utxo_id) {
                        return Err(ExecutorError::TransactionValidity(
                            TransactionValidityError::CoinDoesNotExist(*utxo_id),
                        ))
                    }
                }
                Input::Contract(_) => {
                    // Nothing to validate
                }

                // Always go to other buket if nonce already is used
                Input::MessageCoinSigned(message::MessageCoinSigned {
                    nonce, ..
                })
                | Input::MessageCoinPredicate(message::MessageCoinPredicate {
                    nonce,
                    ..
                })
                | Input::MessageDataSigned(message::MessageDataSigned {
                    nonce, ..
                })
                | Input::MessageDataPredicate(message::MessageDataPredicate {
                    nonce,
                    ..
                }) => {
                    if self.used_messages.contains(nonce) {
                        return Err(ExecutorError::TransactionValidity(
                            TransactionValidityError::MessageDoesNotExist(*nonce),
                        ))
                    }
                }
            }
        }

        if is_blob(&tx) {
            // Blobs can't touch contracts, so we don't worry about them.
            // If inputs are dependent, blob execution will fail with
            // input doesn't exist.
            // If inputs use blobs in the predicates,
            // based on the order of blobs it can fail with predicate invalid, or not.
            self.blobs_bucket.add(tx_id, gas);
            return Ok(());
        }

        let mut index = BucketIndex::Independent;

        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    self.used_coins.insert(*utxo_id);

                    // Always go to other bucket if parent in this block
                    if let Some((_, parent_index)) =
                        self.txs_to_bucket.get_mut(utxo_id.tx_id())
                    {
                        index = BucketIndex::Other;

                        if *parent_index == BucketIndex::Independent {
                            *parent_index = BucketIndex::Other;
                            let parent_gas =
                                self.independent_bucket.remove(utxo_id.tx_id());

                            if let Some(parent_gas) = parent_gas {
                                self.other_buckets.add(*utxo_id.tx_id(), parent_gas);
                            }
                        }
                    }
                }
                Input::Contract(contract) => {
                    // TODO: Filter coinbase contract ?
                    if let Some(creator_tx_id) =
                        self.created_contracts.get(&contract.contract_id)
                    {
                        index = BucketIndex::Other;
                        if let Some((_, parent_index)) =
                            self.txs_to_bucket.get_mut(creator_tx_id)
                        {
                            if *parent_index == BucketIndex::Independent {
                                *parent_index = BucketIndex::Other;
                                let parent_gas =
                                    self.independent_bucket.remove(creator_tx_id);

                                if let Some(parent_gas) = parent_gas {
                                    self.other_buckets.add(*creator_tx_id, parent_gas);
                                }
                            }
                        }
                    }
                }

                // Always go to other buket if nonce already is used
                Input::MessageCoinSigned(message::MessageCoinSigned {
                    nonce, ..
                })
                | Input::MessageCoinPredicate(message::MessageCoinPredicate {
                    nonce,
                    ..
                })
                | Input::MessageDataSigned(message::MessageDataSigned {
                    nonce, ..
                })
                | Input::MessageDataPredicate(message::MessageDataPredicate {
                    nonce,
                    ..
                }) => {
                    self.used_messages.insert(*nonce);
                }
            }
        }

        let outputs = tx.outputs()?;
        for output in outputs {
            match output {
                output::Output::ContractCreated { contract_id, .. } => {
                    self.created_contracts.insert(*contract_id, tx_id);
                }
                _ => {}
            }
        }

        self.txs_to_bucket.insert(tx_id, (tx, index));

        match index {
            BucketIndex::Independent => {
                self.independent_bucket.add(tx_id, gas);
            }
            BucketIndex::Other => {
                self.other_buckets.add(tx_id, gas);
            }
        }

        Ok(())
    }

    /// The last bucket always contains blobs at the end.
    pub fn split_equally(
        mut self,
        number_of_buckets: NonZeroUsize,
    ) -> Vec<(u64, Vec<MaybeCheckedTransaction>)> {
        // One of buckets is reserved (not exclusively) for the `other_bucket`, so subtract 1 from the
        // `number_of_buckets`.
        let number_of_buckets = number_of_buckets.get();
        let number_of_wild_buckets = number_of_buckets.saturating_sub(1) as u64;
        // The last bucket should contain all blobs as the end of all transactions.
        // Blobs at the end avoids potential invalidation of the predicates or transactions
        // after then.
        let mut sorted_buckets = Vec::with_capacity(number_of_buckets);
        // It's certainly a bit too big allocation because some transactions will go to the last bucket used also for
        // `others_transactions` but I don't think it's a big deal and we still win in terms of performance.
        let max_size_bucket = self
            .independent_bucket
            .elements
            .len()
            .saturating_div(number_of_wild_buckets as usize)
            .saturating_add(1);
        for _ in 0..number_of_wild_buckets {
            let gas = 0u64;
            let txs: Vec<MaybeCheckedTransaction> = Vec::with_capacity(max_size_bucket);
            sorted_buckets.push((gas, txs));
        }

        let other_gas = self.other_buckets.gas;
        let other_transactions: Vec<MaybeCheckedTransaction> = self
            .other_buckets
            .elements
            .into_iter()
            .filter_map(|tx_id| self.txs_to_bucket.remove(&tx_id).map(|(tx, _)| tx))
            .collect();
        sorted_buckets.push((other_gas, other_transactions));

        let sorted_independent_txs = self
            .independent_bucket
            .elements
            .into_iter()
            .map(|(tx_id, gas)| (Reverse(gas), tx_id))
            .collect::<BTreeSet<_>>();

        let independent_most_expensive_transactions = sorted_independent_txs.into_iter();

        // Iterate from 0 to N buckets and insert transactions (that are sorted by expensive order) to the buckets following these rules:
        // - Insert in order of bucket ids : 0,1,2,..N,N,N-1,N-2,..0,....
        // - Skip the last bucket until the others doesn't have more gas usage than the `other_transactions`
        // (to avoid filling the last bucket at the same rhythm as the others given the fact that he already contains the `others_transaction`)
        // SAFE: `number_of_buckets` comes from a `NonZeroUsize` so it's always greater than 0
        #[derive(Debug, PartialEq, Eq, Copy, Clone)]
        enum IterationDirection {
            Forward,
            Backward,
        }
        let last_bucket_idx = number_of_buckets - 1;
        let mut current_bucket_idx = 0;
        let mut direction = IterationDirection::Forward;
        // Allow to block iteration on the edges (0 and N) to fill them twice before going to the other side
        let mut iterate_next_time = false;
        // Used to skip the last bucket if the other transactions have more gas than the current other biggest bucket
        let mut most_gas_usage_bucket = 0;
        for (gas, tx_id) in independent_most_expensive_transactions {
            // If we are in the last bucket we change the direction
            if current_bucket_idx == last_bucket_idx {
                iterate_next_time = if iterate_next_time == true {
                    false
                } else {
                    true
                };
                direction = IterationDirection::Backward;
                // If we are in the last bucket and the other transactions have more gas than the current other biggest bucket, we skip the last bucket
                if most_gas_usage_bucket < other_gas {
                    current_bucket_idx = current_bucket_idx.saturating_sub(1);
                }
            }
            // If we are in the first bucket we change the direction
            if current_bucket_idx == 0 {
                iterate_next_time = if iterate_next_time == true {
                    false
                } else {
                    true
                };
                direction = IterationDirection::Forward;
            }
            let txs = sorted_buckets
                .get_mut(current_bucket_idx)
                .expect("Always has items in the `sorted_buckets`; qed");

            if let Some(tx) = self.txs_to_bucket.remove(&tx_id).map(|(tx, _)| tx) {
                txs.1.push(tx);
                txs.0 += gas.0;
            }

            most_gas_usage_bucket = most_gas_usage_bucket.max(txs.0);

            let direction_sign = -1 * ((direction as i32 * 2) - 1);
            let step = direction_sign * (iterate_next_time as i32);
            current_bucket_idx =
                ((current_bucket_idx as i32).saturating_add(step)) as usize;
        }

        // Get latest bucket on the vector
        let last_bucket = sorted_buckets
            .last_mut()
            .expect("Always has items in the `sorted_buckets`; qed");

        last_bucket.0 += self.blobs_bucket.gas;
        self.blobs_bucket.elements.into_iter().for_each(|tx_id| {
            if let Some(tx) = self.txs_to_bucket.remove(&tx_id).map(|(tx, _)| tx) {
                last_bucket.1.push(tx);
            }
        });
        debug_assert_eq!(sorted_buckets.len(), number_of_buckets);
        sorted_buckets
    }
}

fn is_blob(tx: &MaybeCheckedTransaction) -> bool {
    matches!(
        tx,
        MaybeCheckedTransaction::Transaction(Transaction::Blob(_))
    ) || matches!(
        tx,
        MaybeCheckedTransaction::CheckedTransaction(CheckedTransaction::Blob(_), _)
    )
}
