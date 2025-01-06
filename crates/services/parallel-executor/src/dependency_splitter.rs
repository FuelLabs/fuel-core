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
    collections::{
        BTreeMap,
        BTreeSet,
    },
    num::NonZeroUsize,
};

type SequenceNumber = usize;

// TODO: Try to avoid iterating across FxHashMaps and make indexes
// TODO: Try to simplify the logic split_equally to take one transaction for each bucket and then on the opposite side to avoid re-sort

#[derive(Default)]
struct Bucket {
    gas: u64,
    current_sequence_number: SequenceNumber,
    elements: FxHashMap<TxId, (SequenceNumber, u64)>,
}

impl Bucket {
    fn new(txs_len: usize) -> Self {
        Self {
            elements: FxHashMap::with_capacity_and_hasher(txs_len, Default::default()),
            ..Default::default()
        }
    }
}

impl Bucket {
    fn add(&mut self, tx_id: TxId, gas: u64) {
        self.gas += gas;
        self.elements
            .insert(tx_id, (self.current_sequence_number, gas));
        self.current_sequence_number += 1;
    }

    fn remove(&mut self, tx_id: &TxId) -> Option<u64> {
        self.elements.remove(tx_id).map(|(_, gas)| gas)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum BucketIndex {
    Independent,
    Other,
}

pub struct DependencySplitter {
    independent_bucket: Bucket,
    other_buckets: Bucket,
    blobs_bucket: Bucket,
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
            independent_bucket: Bucket::new(txs_len),
            other_buckets: Bucket::new(txs_len),
            blobs_bucket: Bucket::new(txs_len),
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
    ) -> Vec<Vec<MaybeCheckedTransaction>> {
        // The last bucket should contain all blobs as the end of all transactions.
        // Blobs at the end avoids potential invalidation of the predicates or transactions
        // after then.
        let mut sorted_buckets = BTreeMap::new();

        // One of buckets is reserved for the `other_bucket`, so subtract 1 from the
        // `number_of_buckets`.
        let number_of_wild_buckets = number_of_buckets.get().saturating_sub(1) as u64;
        for idx in 0..number_of_wild_buckets {
            let gas = 0u64;
            let txs = BTreeMap::<SequenceNumber, TxId>::new();
            sorted_buckets.insert((gas, idx), txs);
        }

        let other_gas = self.other_buckets.gas;
        let other_transactions = self
            .other_buckets
            .elements
            .into_iter()
            .map(|(tx_id, (seq_num, _))| (seq_num, tx_id))
            .collect();
        sorted_buckets.insert((other_gas, number_of_wild_buckets), other_transactions);

        let sorted_independent_txs = self
            .independent_bucket
            .elements
            .into_iter()
            .map(|(tx_id, (seq_num, gas))| (Reverse(gas), tx_id, seq_num))
            .collect::<BTreeSet<_>>();

        let independent_most_expensive_transactions = sorted_independent_txs.into_iter();

        for (gas, tx_id, seq_num) in independent_most_expensive_transactions {
            let most_empty_bucket = sorted_buckets
                .pop_first()
                .expect("Always has items in the `sorted_buckets`; qed");

            let (total_gas, idx) = most_empty_bucket.0;
            let mut txs = most_empty_bucket.1;

            let new_total_gas = total_gas.saturating_add(gas.0);
            txs.insert(seq_num, tx_id);

            sorted_buckets.insert((new_total_gas, idx), txs);
        }

        let most_empty_bucket = sorted_buckets
            .pop_first()
            .expect("Always has items in the `sorted_buckets`; qed");
        let mut buckets = sorted_buckets
            .into_iter()
            .map(|(_, txs)| {
                txs.into_iter()
                    .filter_map(|(_, tx_id)| {
                        self.txs_to_bucket.remove(&tx_id).map(|(tx, _)| tx)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let mut txs_from_most_empty_bucket = most_empty_bucket
            .1
            .into_iter()
            .filter_map(|(_, tx_id)| self.txs_to_bucket.remove(&tx_id).map(|(tx, _)| tx))
            .collect::<Vec<_>>();
        let sorted_blobs = self
            .blobs_bucket
            .elements
            .into_iter()
            .map(|(tx_id, (seq_num, _))| (seq_num, tx_id))
            .collect::<BTreeMap<_, _>>();
        let blobs = sorted_blobs
            .into_iter()
            .filter_map(|(_, tx_id)| self.txs_to_bucket.remove(&tx_id).map(|(tx, _)| tx));
        txs_from_most_empty_bucket.extend(blobs);
        let bucket_with_blobs = txs_from_most_empty_bucket;

        buckets.push(bucket_with_blobs);
        debug_assert_eq!(buckets.len(), number_of_buckets.get());

        buckets
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
