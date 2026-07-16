//! Integration of the event-driven [`lane_scheduler`] (`rw-lanes-fast`) into
//! `txpool_v2`.
//!
//! This module is the whole seam between the pool and the lane scheduler. It is
//! only active when [`crate::config::Config::lane_scheduler`] is `true` (default
//! `false`), so with the flag off the pool behaves byte-for-byte as before —
//! every hook in [`crate::pool`] is guarded behind `Option::is_some`.
//!
//! # What lives here
//! - [`PoolScheduledTx`]: the pool's transaction adapter implementing
//!   [`ScheduledTransaction`] (id / max_gas / tip / size / contract accesses /
//!   in-pool parents).
//! - [`derive_contract_accesses`]: the fuel-core Read/Write derivation rule.
//! - [`LaneSchedulerState`]: the pool-owned scheduler plus its piggybacked
//!   feedback queue.
//!
//! # The Read/Write derivation rule (user-confirmed)
//! A contract that appears as a transaction INPUT **with a matching contract
//! OUTPUT** (an `Output::Contract` whose `input_index` points back at the
//! contract input) has its state changed → [`Access::Write`]. A contract INPUT
//! **with no matching contract output** is observed but not changed →
//! [`Access::Read`]. A newly created contract (`Output::ContractCreated`) is a
//! [`Access::Write`] on that new contract. fuel-core never emits
//! [`Access::Delta`].

use std::{
    collections::HashSet,
    sync::Arc,
};

use fuel_core_types::{
    fuel_tx::{
        ContractId,
        Input,
        Output,
        TxId,
    },
    services::txpool::{
        ArcPoolTx,
        PoolTransaction,
    },
};

pub use lane_scheduler::{
    Access,
    BatchFeedback,
    BatchId,
    BatchProposal,
    BatchRequest,
    ExecutingContracts,
    LaneScheduler,
    RemovalReason,
    ScheduledTransaction,
    SchedulerConfig,
    WindowContext,
    WorkerBudget,
};

/// The pool's transaction as seen by the lane scheduler. Wraps the shared
/// [`ArcPoolTx`] and pre-computes the two derived pieces the scheduler reads
/// repeatedly: the contract access set (Read/Write) and the in-pool parents.
#[derive(Debug)]
pub struct PoolScheduledTx {
    tx: ArcPoolTx,
    /// Pre-derived contract accesses (see [`derive_contract_accesses`]).
    accesses: Vec<(ContractId, Access)>,
    /// In-pool UTXO/coin parents (transactions that must execute first). The
    /// scheduler tracks readiness itself from these edges.
    parents: Vec<TxId>,
}

impl PoolScheduledTx {
    /// Build the adapter for a stored transaction and its in-pool parent tx ids.
    pub fn new(tx: ArcPoolTx, parents: Vec<TxId>) -> Self {
        let accesses = derive_contract_accesses(&tx);
        Self {
            tx,
            accesses,
            parents,
        }
    }
}

impl ScheduledTransaction for PoolScheduledTx {
    type Id = TxId;
    type ContractId = ContractId;

    fn id(&self) -> TxId {
        self.tx.id()
    }

    fn max_gas(&self) -> u64 {
        self.tx.max_gas()
    }

    fn tip(&self) -> u64 {
        self.tx.tip()
    }

    fn size(&self) -> u64 {
        self.tx.metered_bytes_size() as u64
    }

    fn contract_accesses(&self) -> impl Iterator<Item = (ContractId, Access)> + '_ {
        self.accesses.iter().copied()
    }

    fn parents(&self) -> impl Iterator<Item = TxId> + '_ {
        self.parents.iter().copied()
    }
}

/// Apply the fuel-core Read/Write derivation rule to a pool transaction.
///
/// See the module docs for the rule. Mirrors the contract collection that the
/// parallel-executor's `prepare_transactions_batch` performs, but keeps the
/// Read/Write distinction the executor currently collapses into "every contract
/// input is exclusive".
pub fn derive_contract_accesses(tx: &PoolTransaction) -> Vec<(ContractId, Access)> {
    derive_contract_accesses_from_io(tx.inputs(), tx.outputs())
}

/// The core Read/Write derivation, split out so it can be unit-tested from
/// hand-built input/output vectors without constructing a whole checked
/// transaction.
pub(crate) fn derive_contract_accesses_from_io(
    inputs: &[Input],
    outputs: &[Output],
) -> Vec<(ContractId, Access)> {
    // Input indices that have a matching `Output::Contract` — those inputs are
    // written. `Output::input_index()` is `Some` only for `Output::Contract`.
    let mut written_inputs: HashSet<u16> = HashSet::new();
    for output in outputs {
        if let Some(idx) = output.input_index() {
            written_inputs.insert(idx);
        }
    }

    let mut accesses = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        if let Input::Contract(contract) = input {
            let access = if written_inputs.contains(&(index as u16)) {
                Access::Write
            } else {
                Access::Read
            };
            accesses.push((contract.contract_id, access));
        }
    }

    // Newly created contracts are writes on the fresh contract id.
    for output in outputs {
        if let Output::ContractCreated { contract_id, .. } = output {
            accesses.push((*contract_id, Access::Write));
        }
    }

    accesses
}

/// Build the scheduler's `executing_contracts` (in-flight lock set) from the
/// excluded-contract set the executor supplies in [`crate::Constraints`].
///
/// The current executor↔pool contract only carries a flat set of excluded
/// contract ids without access modes, so every excluded contract is locked as a
/// [`Access::Write`] (conservative — a writer forbids any concurrent access).
/// Once the executor is extended to report per-batch access modes this should
/// distinguish Read-only in-flight locks so concurrent readers can share.
pub fn executing_contracts_from_excluded(
    excluded: &HashSet<ContractId>,
) -> ExecutingContracts<ContractId> {
    let mut executing = ExecutingContracts::new();
    for contract in excluded {
        // TODO(access-modes): the caller cannot yet tell Read from Write for
        // in-flight batches; assume Write (safe). Reader-sharing across
        // in-flight batches is unlocked once modes are threaded through.
        executing.lock(*contract, Access::Write);
    }
    executing
}

/// The pool-owned lane scheduler plus the piggybacked feedback queue.
///
/// Feedback that arrives out-of-band (via the pool_worker feedback message) is
/// buffered here and drained onto the next [`BatchRequest::feedback`] — the
/// primary transport the scheduler documents.
pub struct LaneSchedulerState {
    scheduler: LaneScheduler<PoolScheduledTx>,
    pending_feedback: Vec<BatchFeedback>,
}

impl std::fmt::Debug for LaneSchedulerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaneSchedulerState")
            .field("pending_count", &self.scheduler.pending_count())
            .field("pending_feedback", &self.pending_feedback.len())
            .finish()
    }
}

impl LaneSchedulerState {
    /// Create the scheduler. `block_gas_limit` seeds the exact-windowed-path gas
    /// cap; the per-request `workers` (the executor's live worker budgets)
    /// override the answer shape on every [`Self::next_batches`] call, so the
    /// construction-time `workers` value is only a slice-sizing hint.
    pub fn new(block_gas_limit: u64) -> Self {
        let scheduler = LaneScheduler::new(SchedulerConfig {
            // Per-request `BatchRequest::workers` is the ground truth for the
            // answer shape; this only seeds analytic slice sizing.
            workers: 1,
            block_gas_limit: Some(block_gas_limit),
            ..SchedulerConfig::default()
        });
        Self {
            scheduler,
            pending_feedback: Vec::new(),
        }
    }

    /// Every pooled transaction (ready or waiting on parents).
    pub fn on_transaction(&mut self, tx: ArcPoolTx, parents: Vec<TxId>) {
        self.scheduler
            .on_transaction(Arc::new(PoolScheduledTx::new(tx, parents)));
    }

    /// A transaction leaving the pool without having been scheduled.
    pub fn on_removal(&mut self, id: &TxId, reason: RemovalReason) {
        self.scheduler.on_removal(id, reason);
    }

    /// Confirm which proposed transactions were actually transferred.
    pub fn on_dispatched(&mut self, batch_id: BatchId, taken: &[TxId]) {
        self.scheduler.on_dispatched(batch_id, taken);
    }

    /// Buffer completion/overhead feedback for the next request (piggyback).
    pub fn queue_feedback(&mut self, feedback: BatchFeedback) {
        self.pending_feedback.push(feedback);
    }

    /// Answer a single-worker batch request built from the executor
    /// constraints, draining any buffered feedback onto it. Returns the ordered
    /// proposal (tx ids) for the one worker slot, if any.
    ///
    /// The pool maps one executor "ask" to one worker budget: the current
    /// executor asks once per idle worker, so a single [`WorkerBudget`] per call
    /// matches the live protocol. `size` / `tx_count` bound the proposal too.
    pub fn next_single_batch(
        &mut self,
        max_gas: u64,
        max_txs: u64,
        max_block_size: u64,
        excluded: &HashSet<ContractId>,
    ) -> Option<BatchProposal<TxId>> {
        let request = BatchRequest {
            workers: vec![WorkerBudget {
                gas: max_gas,
                size: max_block_size,
                tx_count: max_txs,
            }],
            executing_contracts: executing_contracts_from_excluded(excluded),
            feedback: std::mem::take(&mut self.pending_feedback),
            // Always `Some` to force the exact windowed path (the fast path is a
            // simulation-only optimization gated behind the `unconstrained-fast`
            // feature, which we deliberately do not enable). A finite
            // `block_gas_remaining` also selects the exact path.
            window: Some(WindowContext {
                now: 0,
                block_gas_remaining: max_gas,
                window_fit_gas: max_gas,
                deadline: None,
                block_start: 0,
            }),
        };
        self.scheduler.next_batches(&request).into_iter().next()
    }

    #[cfg(test)]
    pub fn pending_count(&self) -> usize {
        self.scheduler.pending_count()
    }

    /// Number of dispatched batches still awaiting completion feedback. Drops by
    /// one when a `completed` [`BatchFeedback`] for a batch is applied — lets a
    /// test observe that the executor→pool feedback round-trip landed.
    #[cfg(test)]
    pub fn in_flight_batches(&self) -> usize {
        self.scheduler.in_flight_batches()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::fuel_tx::{
        Bytes32,
        Output,
        TxPointer,
        UtxoId,
    };

    fn contract_id(byte: u8) -> ContractId {
        ContractId::from([byte; 32])
    }

    fn contract_input(contract: ContractId) -> Input {
        Input::contract(
            UtxoId::new(Default::default(), 0),
            Bytes32::zeroed(),
            Bytes32::zeroed(),
            TxPointer::default(),
            contract,
        )
    }

    #[test]
    fn contract_input_with_matching_output_is_write() {
        let c = contract_id(1);
        let inputs = vec![contract_input(c)];
        // Output::Contract referencing input index 0 → the input is written.
        let outputs = vec![Output::contract(0, Bytes32::zeroed(), Bytes32::zeroed())];

        let accesses = derive_contract_accesses_from_io(&inputs, &outputs);
        assert_eq!(accesses, vec![(c, Access::Write)]);
    }

    #[test]
    fn contract_input_without_matching_output_is_read() {
        let c = contract_id(2);
        let inputs = vec![contract_input(c)];
        // No contract output → the contract is only observed → Read.
        let outputs = vec![];

        let accesses = derive_contract_accesses_from_io(&inputs, &outputs);
        assert_eq!(accesses, vec![(c, Access::Read)]);
    }

    #[test]
    fn contract_created_output_is_write() {
        let created = contract_id(3);
        let inputs = vec![];
        let outputs = vec![Output::contract_created(created, Bytes32::zeroed())];

        let accesses = derive_contract_accesses_from_io(&inputs, &outputs);
        assert_eq!(accesses, vec![(created, Access::Write)]);
    }

    #[test]
    fn mixed_read_write_and_created() {
        let written = contract_id(1);
        let read = contract_id(2);
        let created = contract_id(3);
        // input 0 = written contract, input 1 = read-only contract.
        let inputs = vec![contract_input(written), contract_input(read)];
        // Only input index 0 has a matching contract output.
        let outputs = vec![
            Output::contract(0, Bytes32::zeroed(), Bytes32::zeroed()),
            Output::contract_created(created, Bytes32::zeroed()),
        ];

        let accesses = derive_contract_accesses_from_io(&inputs, &outputs);
        assert_eq!(
            accesses,
            vec![
                (written, Access::Write),
                (read, Access::Read),
                (created, Access::Write),
            ]
        );
    }

    #[test]
    fn no_contract_access_for_pure_coin_tx() {
        let accesses = derive_contract_accesses_from_io(&[], &[]);
        assert!(accesses.is_empty());
    }
}
