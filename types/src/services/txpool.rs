//! Types for interoperability with the txpool service

use crate::{
    blockchain::primitives::BlockId,
    fuel_asm::Word,
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
        },
        Cacheable,
        Chargeable,
        Checked,
        ConsensusParameters,
        Create,
        Input,
        Output,
        Script,
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::Bytes32,
    fuel_vm::{
        Interpreter,
        PredicateStorage,
        ProgramState,
    },
};
use std::{
    ops::Deref,
    sync::Arc,
};
use tai64::Tai64;

/// Pool transaction wrapped in an Arc for thread-safe sharing
pub type ArcPoolTx = Arc<PoolTransaction>;

/// Transaction type used by the transaction pool. Transaction pool supports not
/// all `fuel_tx::Transaction` variants.
#[derive(Debug, Eq, PartialEq)]
pub enum PoolTransaction {
    /// Script
    Script(Checked<Script>),
    /// Create
    Create(Checked<Create>),
}

impl Chargeable for PoolTransaction {
    fn price(&self) -> Word {
        match self {
            PoolTransaction::Script(script) => script.transaction().price(),
            PoolTransaction::Create(create) => create.transaction().price(),
        }
    }

    fn limit(&self) -> Word {
        match self {
            PoolTransaction::Script(script) => script.transaction().limit(),
            PoolTransaction::Create(create) => create.transaction().limit(),
        }
    }

    fn metered_bytes_size(&self) -> usize {
        match self {
            PoolTransaction::Script(script) => script.transaction().metered_bytes_size(),
            PoolTransaction::Create(create) => create.transaction().metered_bytes_size(),
        }
    }
}

impl UniqueIdentifier for PoolTransaction {
    fn id(&self) -> Bytes32 {
        match self {
            PoolTransaction::Script(script) => script.transaction().id(),
            PoolTransaction::Create(create) => create.transaction().id(),
        }
    }
}

#[allow(missing_docs)]
impl PoolTransaction {
    pub fn is_computed(&self) -> bool {
        match self {
            PoolTransaction::Script(script) => script.transaction().is_computed(),
            PoolTransaction::Create(create) => create.transaction().is_computed(),
        }
    }

    pub fn inputs(&self) -> &Vec<Input> {
        match self {
            PoolTransaction::Script(script) => script.transaction().inputs(),
            PoolTransaction::Create(create) => create.transaction().inputs(),
        }
    }

    pub fn outputs(&self) -> &Vec<Output> {
        match self {
            PoolTransaction::Script(script) => script.transaction().outputs(),
            PoolTransaction::Create(create) => create.transaction().outputs(),
        }
    }

    pub fn max_gas(&self) -> Word {
        match self {
            PoolTransaction::Script(script) => script.metadata().fee.max_gas(),
            PoolTransaction::Create(create) => create.metadata().fee.max_gas(),
        }
    }

    pub fn check_predicates(&self, params: ConsensusParameters) -> bool {
        match self {
            PoolTransaction::Script(script) => {
                Interpreter::<PredicateStorage>::check_predicates(script.clone(), params)
            }
            PoolTransaction::Create(create) => {
                Interpreter::<PredicateStorage>::check_predicates(create.clone(), params)
            }
        }
    }
}

impl From<&PoolTransaction> for Transaction {
    fn from(tx: &PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(script) => {
                Transaction::Script(script.transaction().clone())
            }
            PoolTransaction::Create(create) => {
                Transaction::Create(create.transaction().clone())
            }
        }
    }
}

impl From<Checked<Script>> for PoolTransaction {
    fn from(checked: Checked<Script>) -> Self {
        Self::Script(checked)
    }
}

impl From<Checked<Create>> for PoolTransaction {
    fn from(checked: Checked<Create>) -> Self {
        Self::Create(checked)
    }
}

/// The `removed` field contains the list of removed transactions during the insertion
/// of the `inserted` transaction.
#[derive(Debug)]
pub struct InsertionResult {
    /// This was inserted
    pub inserted: ArcPoolTx,
    /// These were removed during the insertion
    pub removed: Vec<ArcPoolTx>,
}

/// Information of a transaction fetched from the txpool
#[derive(Debug, Clone)]
pub struct TxInfo {
    tx: ArcPoolTx,
    submitted_time: Tai64,
}

#[allow(missing_docs)]
impl TxInfo {
    pub fn new(tx: ArcPoolTx) -> Self {
        Self {
            tx,
            submitted_time: Tai64::now(),
        }
    }

    pub fn tx(&self) -> &ArcPoolTx {
        &self.tx
    }

    pub fn submitted_time(&self) -> Tai64 {
        self.submitted_time
    }
}

impl Deref for TxInfo {
    type Target = ArcPoolTx;
    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}

/// The status of the transaction during its life from the tx pool until the block.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction was submitted into the txpool
    Submitted {
        /// Timestamp of submission into the txpool
        time: Tai64,
    },
    /// Transaction was successfully included in a block
    Success {
        /// Included in this block
        block_id: BlockId,
        /// Time when the block was generated
        time: Tai64,
        /// Result of executing the transaction for scripts
        result: Option<ProgramState>,
    },
    /// Transaction was squeezed of the txpool
    SqueezedOut {
        /// Why this happened
        reason: String,
    },
    /// Transaction was included in a block, but the exection was reverted
    Failed {
        /// Included in this block
        block_id: BlockId,
        /// Time when the block was generated
        time: Tai64,
        /// Why this happened
        reason: String,
        /// Result of executing the transaction for scripts
        result: Option<ProgramState>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)] // TODO: This type is likely removed soon
pub enum TxStatus {
    /// Submitted into txpool.
    Submitted,
    /// Transaction has either been:
    /// - successfully executed and included in a block.
    /// - failed to execute and state changes reverted
    Completed,
    /// removed from txpool.
    SqueezedOut { reason: String },
}
