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
    },
    fuel_vm::checked_transaction::CheckedTransaction,
    services::executor::{
        Error as ExecutorError,
        Result as ExecutorResult,
    },
};
use fuel_core_upgradable_executor::native_executor::ports::{
    MaybeCheckedTransaction,
    TransactionExt,
};
use rustc_hash::FxHashMap;
use std::{
    cmp::Reverse,
    num::NonZeroUsize,
};

// TODO: Try to replace txid with global sequence id to lower copy times

type Gas = u64;

#[derive(Default)]
struct OrderedBucket {
    gas: Gas,
    elements: Vec<MaybeCheckedTransaction>,
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
    fn add(&mut self, tx: MaybeCheckedTransaction, gas: Gas) {
        self.gas += gas;
        self.elements.push(tx);
    }
}

#[derive(Default)]
pub struct UnorderedBucket {
    gas: Gas,
    // Gas used to be used when passed to an other bucket and to order transactions afterwards
    // bool is used to know if the transaction is still in the bucket
    element_ids: Vec<(TxId, Gas, bool)>,
    elements: FxHashMap<TxId, MaybeCheckedTransaction>,
}

impl UnorderedBucket {
    fn new(txs_len: usize) -> Self {
        Self {
            elements: FxHashMap::with_capacity_and_hasher(txs_len, Default::default()),
            element_ids: Vec::with_capacity(txs_len),
            ..Default::default()
        }
    }
}

impl UnorderedBucket {
    fn add(&mut self, tx_id: TxId, tx: MaybeCheckedTransaction, gas: Gas) {
        self.gas += gas;
        self.element_ids.push((tx_id, gas, true));
        self.elements.insert(tx_id, tx);
    }

    fn remove(&mut self, tx_id: &TxId) -> Option<(MaybeCheckedTransaction, Gas)> {
        let (_, gas, exists) =
            self.element_ids.iter_mut().find(|(id, _, _)| id == tx_id)?;
        *exists = false;
        self.gas -= *gas;
        let tx = self.elements.remove(tx_id).expect("Transaction should be in the `elements` because it's in the `element_ids`; qed");
        Some((tx, *gas))
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
    txs_to_bucket: FxHashMap<TxId, BucketIndex>,
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

        if is_blob(&tx) {
            // Blobs can't touch contracts, so we don't worry about them.
            // If inputs are dependent, blob execution will fail with
            // input doesn't exist.
            // If inputs use blobs in the predicates,
            // based on the order of blobs it can fail with predicate invalid, or not.
            self.blobs_bucket.add(tx, gas);
            return Ok(());
        }

        let mut index = BucketIndex::Independent;

        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {

                    // Always go to other bucket if parent in this block
                    if let Some(parent_index) =
                        self.txs_to_bucket.get_mut(utxo_id.tx_id())
                    {
                        index = BucketIndex::Other;

                        if *parent_index == BucketIndex::Independent {
                            *parent_index = BucketIndex::Other;
                            let parent_data =
                                self.independent_bucket.remove(utxo_id.tx_id());

                            if let Some((parent_tx, parent_gas)) = parent_data {
                                self.other_buckets.add(parent_tx, parent_gas);
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
                        if let Some(parent_index) =
                            self.txs_to_bucket.get_mut(creator_tx_id)
                        {
                            if *parent_index == BucketIndex::Independent {
                                *parent_index = BucketIndex::Other;
                                let parent_data =
                                    self.independent_bucket.remove(creator_tx_id);

                                if let Some((parent_tx, parent_gas)) = parent_data {
                                    self.other_buckets.add(parent_tx, parent_gas);
                                }
                            }
                        }
                    }
                }

                // Always go to other buket if nonce already is used
                Input::MessageCoinSigned(message::MessageCoinSigned {
                    ..
                })
                | Input::MessageCoinPredicate(message::MessageCoinPredicate {
                    ..
                })
                | Input::MessageDataSigned(message::MessageDataSigned {
                    ..
                })
                | Input::MessageDataPredicate(message::MessageDataPredicate {
                    ..
                }) => {
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

        self.txs_to_bucket.insert(tx_id, index);

        match index {
            BucketIndex::Independent => {
                self.independent_bucket.add(tx_id, tx, gas);
            }
            BucketIndex::Other => {
                self.other_buckets.add(tx, gas);
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
        sorted_buckets.push((self.other_buckets.gas, self.other_buckets.elements));

        // Sort independent transactions by gas usage in descending order
        self.independent_bucket
            .element_ids
            .sort_by_key(|(tx_id, gas, exists)| (*exists, Reverse(*gas), *tx_id));

        let independent_most_expensive_transactions =
            self.independent_bucket.element_ids.into_iter();

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
        for (tx_id, gas, exists) in independent_most_expensive_transactions {
            // All the transactions that have been moved to `others_transactions` should be at the end
            // so we can break the loop if we reach them
            if !exists {
                break;
            }
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
            let tx = self.independent_bucket.elements.remove(&tx_id).expect(
                "Transaction should be in the `independent_bucket` because it's sorted by gas usage; qed",
            );
            txs.1.push(tx);
            txs.0 += gas;

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
        last_bucket.1.extend(self.blobs_bucket.elements);

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
