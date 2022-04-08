use crate::database::transactional::DatabaseTransaction;
use crate::database::Database;
use crate::schema::scalars::U64;
use async_graphql::{Context, Object, SchemaBuilder, ID};
use fuel_vm::{consts, prelude::*};
use futures::lock::Mutex;
use std::{collections::HashMap, io, sync};
use tracing::{debug, trace};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct ConcreteStorage {
    vm: HashMap<ID, Interpreter<Database>>,
    tx: HashMap<ID, Vec<Transaction>>,
    db: HashMap<ID, DatabaseTransaction>,
}

impl ConcreteStorage {
    pub fn register(&self, id: &ID, register: RegisterId) -> Option<Word> {
        self.vm
            .get(id)
            .and_then(|vm| vm.registers().get(register).copied())
    }

    pub fn memory(&self, id: &ID, start: usize, size: usize) -> Option<&[u8]> {
        let (end, overflow) = start.overflowing_add(size);
        if overflow || end > consts::VM_MAX_RAM as usize {
            return None;
        }

        self.vm.get(id).map(|vm| &vm.memory()[start..end])
    }

    pub fn init(
        &mut self,
        txs: &[Transaction],
        storage: DatabaseTransaction,
    ) -> Result<ID, InterpreterError> {
        let id = Uuid::new_v4();
        let id = ID::from(id);

        let tx = txs.first().cloned().unwrap_or_default();
        self.tx
            .get_mut(&id)
            .map(|tx| tx.extend_from_slice(txs))
            .unwrap_or_else(|| {
                self.tx.insert(id.clone(), txs.to_owned());
            });

        let mut vm = Interpreter::with_storage(storage.as_ref().clone());
        vm.transact(tx)?;
        self.vm.insert(id.clone(), vm);
        self.db.insert(id.clone(), storage);

        Ok(id)
    }

    pub fn kill(&mut self, id: &ID) -> bool {
        self.tx.remove(id);
        self.vm.remove(id);
        self.db.remove(id).is_some()
    }

    pub fn reset(&mut self, id: &ID, storage: DatabaseTransaction) -> Result<(), InterpreterError> {
        let tx = self
            .tx
            .get(id)
            .and_then(|tx| tx.first())
            .cloned()
            .unwrap_or_default();

        let mut vm = Interpreter::with_storage(storage.as_ref().clone());
        vm.transact(tx)?;
        self.vm.insert(id.clone(), vm).ok_or_else(|| {
            InterpreterError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "The VM instance was not found",
            ))
        })?;
        self.db.insert(id.clone(), storage);
        Ok(())
    }

    pub fn exec(&mut self, id: &ID, op: Opcode) -> Result<(), InterpreterError> {
        self.vm
            .get_mut(id)
            .map(|vm| vm.instruction(op.into()))
            .transpose()?
            .map(|_| ())
            .ok_or_else(|| {
                InterpreterError::Io(io::Error::new(
                    io::ErrorKind::NotFound,
                    "The VM instance was not found",
                ))
            })
    }
}

pub type GraphStorage = sync::Arc<Mutex<ConcreteStorage>>;

#[derive(Default)]
pub struct DapQuery;
#[derive(Default)]
pub struct DapMutation;

pub fn init<Q, M, S>(schema: SchemaBuilder<Q, M, S>) -> SchemaBuilder<Q, M, S> {
    schema.data(GraphStorage::default())
}

#[Object]
impl DapQuery {
    async fn register(
        &self,
        ctx: &Context<'_>,
        id: ID,
        register: U64,
    ) -> async_graphql::Result<U64> {
        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .register(&id, register.into())
            .ok_or_else(|| async_graphql::Error::new("Invalid register identifier"))
            .map(|val| val.into())
    }

    async fn memory(
        &self,
        ctx: &Context<'_>,
        id: ID,
        start: U64,
        size: U64,
    ) -> async_graphql::Result<String> {
        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .memory(&id, start.into(), size.into())
            .ok_or_else(|| async_graphql::Error::new("Invalid memory range"))
            .and_then(|mem| Ok(serde_json::to_string(mem)?))
    }
}

#[Object]
impl DapMutation {
    async fn start_session(&self, ctx: &Context<'_>) -> async_graphql::Result<ID> {
        trace!("Initializing new interpreter");

        let db = ctx.data_unchecked::<Database>();

        let id = ctx
            .data_unchecked::<GraphStorage>()
            .lock()
            .await
            .init(&[], db.transaction())?;

        debug!("Session {:?} initialized", id);

        Ok(id)
    }

    async fn end_session(&self, ctx: &Context<'_>, id: ID) -> bool {
        let existed = ctx.data_unchecked::<GraphStorage>().lock().await.kill(&id);

        debug!("Session {:?} dropped with result {}", id, existed);

        existed
    }

    async fn reset(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<bool> {
        let db = ctx.data_unchecked::<Database>();

        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .reset(&id, db.transaction())?;

        debug!("Session {:?} was reset", id);

        Ok(true)
    }

    async fn execute(&self, ctx: &Context<'_>, id: ID, op: String) -> async_graphql::Result<bool> {
        trace!("Execute encoded op {}", op);

        let op: Opcode = serde_json::from_str(op.as_str())?;

        trace!("Op decoded to {:?}", op);

        let storage = ctx.data_unchecked::<GraphStorage>().clone();
        let result = storage.lock().await.exec(&id, op).is_ok();

        debug!("Op {:?} executed with result {}", op, result);

        Ok(result)
    }

    async fn set_breakpoint(
        &self,
        ctx: &Context<'_>,
        id: ID,
        breakpoint: self::gql_types::Breakpoint,
    ) -> async_graphql::Result<bool> {
        trace!("Continue exectuon of VM {:?}", id);

        let mut locked = ctx.data_unchecked::<GraphStorage>().lock().await;
        let vm = locked.vm.get_mut(&id).expect("TODO: error: VM not found");
        vm.set_breakpoint(breakpoint.into());
        Ok(true)
    }

    async fn start_tx(
        &self,
        ctx: &Context<'_>,
        id: ID,
        tx: self::gql_types::Transaction,
    ) -> async_graphql::Result<Option<self::gql_types::OutputBreakpoint>> {
        trace!("Spawning a new VM instance");
        let tx: Transaction = tx.into();

        let mut locked = ctx.data_unchecked::<GraphStorage>().lock().await;
        let vm = locked.vm.get_mut(&id).expect("TODO: error: VM not found");
        let state_ref = vm.transact(tx).expect("TODO: error: transact");

        // TODO: persist result on success

        Ok(state_ref.state().debug_ref().and_then(|d| match d {
            DebugEval::Continue => None,
            DebugEval::Breakpoint(bp) => Some(bp.into()),
        }))
    }

    async fn continue_tx(&self, ctx: &Context<'_>, id: ID)
        -> async_graphql::Result<Option<self::gql_types::OutputBreakpoint>> {
        trace!("Continue exectuon of VM {:?}", id);

        let mut locked = ctx.data_unchecked::<GraphStorage>().lock().await;
        let vm = locked.vm.get_mut(&id).expect("TODO: error: VM not found");
        let state = vm.resume().expect("Failed to resume");

        dbg!(state);

        // TODO: persist result on success

        Ok(state.debug_ref().and_then(|d| match d {
            DebugEval::Continue => None,
            DebugEval::Breakpoint(bp) => Some(bp.into()),
        }))
    }
}

mod gql_types {
    //! GraphQL type wrappers
    use async_graphql::*;

    use fuel_tx::{
        Input as FuelTransactionInput, Metadata as FuelTransactionMetadata,
        Output as FuelTransactionOutput, StorageSlot as FuelStorageSlot,
        Transaction as FuelTransaction, UtxoId as FuelUtxoId,
    };
    use fuel_types::Word;
    use fuel_vm::prelude::Breakpoint as FuelBreakpoint;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, InputObject)]
    pub struct Bytes32 {
        value: [u8; 32],
    }
    impl From<fuel_types::Bytes32> for Bytes32 {
        fn from(id: fuel_types::Bytes32) -> Self {
            Self { value: *id }
        }
    }
    impl From<Bytes32> for fuel_types::Bytes32 {
        fn from(id: Bytes32) -> Self {
            Self::from(id.value)
        }
    }
    impl From<[u8; 32]> for Bytes32 {
        fn from(value: [u8; 32]) -> Self {
            Self { value }
        }
    }
    impl core::ops::Deref for Bytes32 {
        type Target = [u8; 32];

        fn deref(&self) -> &[u8; 32] {
            &self.value
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct ContractId {
        value: [u8; 32],
    }
    impl From<&fuel_types::ContractId> for ContractId {
        fn from(id: &fuel_types::ContractId) -> Self {
            Self { value: **id }
        }
    }
    impl From<fuel_types::ContractId> for ContractId {
        fn from(id: fuel_types::ContractId) -> Self {
            Self { value: *id }
        }
    }
    impl From<ContractId> for fuel_types::ContractId {
        fn from(id: ContractId) -> Self {
            Self::from(id.value)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SimpleObject)]
    pub struct OutputContractId {
        value: [u8; 32],
    }
    impl From<&fuel_types::ContractId> for OutputContractId {
        fn from(id: &fuel_types::ContractId) -> Self {
            Self { value: **id }
        }
    }
    impl From<fuel_types::ContractId> for OutputContractId {
        fn from(id: fuel_types::ContractId) -> Self {
            Self { value: *id }
        }
    }
    impl From<OutputContractId> for fuel_types::ContractId {
        fn from(id: OutputContractId) -> Self {
            Self::from(id.value)
        }
    }

    #[derive(Debug, Clone, Copy, InputObject)]
    pub struct Breakpoint {
        contract: ContractId,
        pc: Word,
    }
    impl From<&FuelBreakpoint> for Breakpoint {
        fn from(bp: &FuelBreakpoint) -> Self {
            Self {
                contract: bp.contract().into(),
                pc: bp.pc(),
            }
        }
    }
    impl From<Breakpoint> for FuelBreakpoint {
        fn from(bp: Breakpoint) -> Self {
            Self::new(bp.contract.into(), bp.pc)
        }
    }

    #[derive(Debug, Clone, Copy, SimpleObject)]
    pub struct OutputBreakpoint {
        contract: OutputContractId,
        pc: Word,
    }
    impl From<&FuelBreakpoint> for OutputBreakpoint {
        fn from(bp: &FuelBreakpoint) -> Self {
            Self {
                contract: bp.contract().into(),
                pc: bp.pc(),
            }
        }
    }
    impl From<OutputBreakpoint> for FuelBreakpoint {
        fn from(bp: OutputBreakpoint) -> Self {
            Self::new(bp.contract.into(), bp.pc)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct AssetId {
        value: Bytes32,
    }
    impl From<fuel_types::AssetId> for AssetId {
        fn from(v: fuel_types::AssetId) -> Self {
            Self {
                value: Bytes32::from(*v),
            }
        }
    }
    impl From<AssetId> for fuel_types::AssetId {
        fn from(v: AssetId) -> Self {
            Self::from(*v.value)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct Salt {
        value: Bytes32,
    }
    impl From<fuel_types::Salt> for Salt {
        fn from(v: fuel_types::Salt) -> Self {
            Self {
                value: Bytes32::from(*v),
            }
        }
    }
    impl From<Salt> for fuel_types::Salt {
        fn from(v: Salt) -> Self {
            Self::from(*v.value)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct Address {
        value: Bytes32,
    }
    impl From<fuel_types::Address> for Address {
        fn from(v: fuel_types::Address) -> Self {
            Self {
                value: Bytes32::from(*v),
            }
        }
    }
    impl From<Address> for fuel_types::Address {
        fn from(v: Address) -> Self {
            Self::from(*v.value)
        }
    }

    #[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, InputObject)]
    pub struct UtxoId {
        /// transaction id
        tx_id: Bytes32,
        /// output index
        output_index: u8,
    }
    impl From<FuelUtxoId> for UtxoId {
        fn from(id: FuelUtxoId) -> Self {
            Self {
                tx_id: (*id.tx_id()).into(),
                output_index: id.output_index(),
            }
        }
    }
    impl From<UtxoId> for FuelUtxoId {
        fn from(v: UtxoId) -> Self {
            Self::new(v.tx_id.into(), v.output_index)
        }
    }

    /// Invariant: exactly one of the fields should be set
    #[derive(Debug, Default, Clone, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionInput {
        coin: Option<TransactionInputCoin>,
        contract: Option<TransactionInputContract>,
    }
    impl TransactionInput {
        fn to_enum(self) -> Result<TransactionInputEnum, ()> {
            let field_count = [self.coin.is_some(), self.contract.is_some()]
                .iter()
                .filter(|x| **x)
                .count();
            if field_count != 1 {
                return Err(());
            }

            Ok(if let Some(c) = self.coin {
                TransactionInputEnum::Coin(c)
            } else if let Some(c) = self.contract {
                TransactionInputEnum::Contract(c)
            } else {
                unreachable!();
            })
        }
    }
    impl From<TransactionInput> for FuelTransactionInput {
        fn from(tx: TransactionInput) -> Self {
            match tx
                .to_enum()
                .expect("Cannot convert: Invalid source type state")
            {
                TransactionInputEnum::Coin(TransactionInputCoin {
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    witness_index,
                    maturity,
                    predicate,
                    predicate_data,
                }) => FuelTransactionInput::Coin {
                    utxo_id: utxo_id.into(),
                    owner: owner.into(),
                    amount,
                    asset_id: asset_id.into(),
                    witness_index,
                    maturity,
                    predicate,
                    predicate_data,
                },
                TransactionInputEnum::Contract(TransactionInputContract {
                    utxo_id,
                    balance_root,
                    state_root,
                    contract_id,
                }) => FuelTransactionInput::Contract {
                    utxo_id: utxo_id.into(),
                    balance_root: balance_root.into(),
                    state_root: state_root.into(),
                    contract_id: contract_id.into(),
                },
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum TransactionInputEnum {
        Coin(TransactionInputCoin),
        Contract(TransactionInputContract),
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, InputObject)]

    pub struct TransactionInputCoin {
        utxo_id: UtxoId,
        owner: Address,
        amount: Word,
        asset_id: AssetId,
        witness_index: u8,
        maturity: Word,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, InputObject)]

    pub struct TransactionInputContract {
        utxo_id: UtxoId,
        balance_root: Bytes32,
        state_root: Bytes32,
        contract_id: ContractId,
    }

    /// Invariant: exactly one of the fields should be set
    #[derive(Debug, Clone, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionOutput {
        coin: Option<TransactionOutputCoin>,
        contract: Option<TransactionOutputContract>,
        withdrawal: Option<TransactionOutputWithdrawal>,
        change: Option<TransactionOutputChange>,
        variable: Option<TransactionOutputVariable>,
        contract_created: Option<TransactionOutputContractCreated>,
    }
    impl TransactionOutput {
        fn to_enum(self) -> Result<TransactionOutputEnum, ()> {
            let field_count = [
                self.coin.is_some(),
                self.contract.is_some(),
                self.withdrawal.is_some(),
                self.change.is_some(),
                self.variable.is_some(),
                self.contract_created.is_some(),
            ]
            .iter()
            .filter(|x| **x)
            .count();

            if field_count != 1 {
                return Err(());
            }

            Ok(if let Some(c) = self.coin {
                TransactionOutputEnum::Coin(c)
            } else if let Some(c) = self.contract {
                TransactionOutputEnum::Contract(c)
            } else if let Some(c) = self.withdrawal {
                TransactionOutputEnum::Withdrawal(c)
            } else if let Some(c) = self.change {
                TransactionOutputEnum::Change(c)
            } else if let Some(c) = self.variable {
                TransactionOutputEnum::Variable(c)
            } else if let Some(c) = self.contract_created {
                TransactionOutputEnum::ContractCreated(c)
            } else {
                unreachable!();
            })
        }
    }
    impl From<TransactionOutput> for FuelTransactionOutput {
        fn from(tx: TransactionOutput) -> Self {
            match tx
                .to_enum()
                .expect("Cannot convert: Invalid source type state")
            {
                TransactionOutputEnum::Coin(TransactionOutputCoin {
                    to,
                    amount,
                    asset_id,
                }) => FuelTransactionOutput::Coin {
                    to: to.into(),
                    amount,
                    asset_id: asset_id.into(),
                },
                TransactionOutputEnum::Contract(TransactionOutputContract {
                    input_index,
                    balance_root,
                    state_root,
                }) => FuelTransactionOutput::Contract {
                    input_index,
                    balance_root: balance_root.into(),
                    state_root: state_root.into(),
                },
                TransactionOutputEnum::Withdrawal(TransactionOutputWithdrawal {
                    to,
                    amount,
                    asset_id,
                }) => FuelTransactionOutput::Withdrawal {
                    to: to.into(),
                    amount,
                    asset_id: asset_id.into(),
                },
                TransactionOutputEnum::Change(TransactionOutputChange {
                    to,
                    amount,
                    asset_id,
                }) => FuelTransactionOutput::Change {
                    to: to.into(),
                    amount,
                    asset_id: asset_id.into(),
                },
                TransactionOutputEnum::Variable(TransactionOutputVariable {
                    to,
                    amount,
                    asset_id,
                }) => FuelTransactionOutput::Variable {
                    to: to.into(),
                    amount,
                    asset_id: asset_id.into(),
                },
                TransactionOutputEnum::ContractCreated(TransactionOutputContractCreated {
                    contract_id,
                    state_root,
                }) => FuelTransactionOutput::ContractCreated {
                    contract_id: contract_id.into(),
                    state_root: state_root.into(),
                },
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum TransactionOutputEnum {
        Coin(TransactionOutputCoin),
        Contract(TransactionOutputContract),
        Withdrawal(TransactionOutputWithdrawal),
        Change(TransactionOutputChange),
        Variable(TransactionOutputVariable),
        ContractCreated(TransactionOutputContractCreated),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionOutputCoin {
        to: Address,
        amount: Word,
        asset_id: AssetId,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionOutputContract {
        input_index: u8,
        balance_root: Bytes32,
        state_root: Bytes32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionOutputWithdrawal {
        to: Address,
        amount: Word,
        asset_id: AssetId,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionOutputChange {
        to: Address,
        amount: Word,
        asset_id: AssetId,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionOutputVariable {
        to: Address,
        amount: Word,
        asset_id: AssetId,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionOutputContractCreated {
        contract_id: ContractId,
        state_root: Bytes32,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, InputObject)]
    pub struct TransactionMetadata {
        id: Bytes32,
        script_data_offset: Option<usize>,
        input_coin_predicate_offset: Vec<Option<usize>>,
        inputs_offset: Vec<usize>,
        outputs_offset: Vec<usize>,
        witnesses_offset: Vec<usize>,
    }
    impl From<TransactionMetadata> for FuelTransactionMetadata {
        fn from(tx_meta: TransactionMetadata) -> Self {
            Self::new(
                tx_meta.id.into(),
                tx_meta.script_data_offset,
                tx_meta.input_coin_predicate_offset,
                tx_meta.inputs_offset,
                tx_meta.outputs_offset,
                tx_meta.witnesses_offset,
            )
        }
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq, Hash, InputObject)]
    pub struct StorageSlot {
        key: Bytes32,
        value: Bytes32,
    }
    impl From<FuelStorageSlot> for StorageSlot {
        fn from(slot: FuelStorageSlot) -> Self {
            Self {
                key: (*slot.key()).into(),
                value: (*slot.value()).into(),
            }
        }
    }
    impl From<StorageSlot> for FuelStorageSlot {
        fn from(v: StorageSlot) -> Self {
            Self::new(v.key.into(), v.value.into())
        }
    }

    /// Invariant: exactly one of the fields should be set
    #[derive(Debug, Default, Clone, PartialEq, Eq, InputObject)]
    pub struct Transaction {
        script: Option<TransactionScript>,
        create: Option<TransactionCreate>,
    }
    impl Transaction {
        fn to_enum(self) -> Result<TransactionEnum, ()> {
            let field_count = [self.script.is_some(), self.create.is_some()]
                .iter()
                .filter(|x| **x)
                .count();
            if field_count != 1 {
                return Err(());
            }

            Ok(if let Some(c) = self.script {
                TransactionEnum::Script(c)
            } else if let Some(c) = self.create {
                TransactionEnum::Create(c)
            } else {
                unreachable!();
            })
        }
    }
    impl From<Transaction> for FuelTransaction {
        fn from(tx: Transaction) -> Self {
            match tx
                .to_enum()
                .expect("Cannot convert: Invalid source type state")
            {
                TransactionEnum::Script(TransactionScript {
                    gas_price,
                    gas_limit,
                    byte_price,
                    maturity,
                    receipts_root,
                    script,
                    script_data,
                    inputs,
                    outputs,
                    witnesses,
                    metadata,
                }) => FuelTransaction::Script {
                    gas_price,
                    gas_limit,
                    byte_price,
                    maturity,
                    receipts_root: receipts_root.into(),
                    script,
                    script_data,
                    inputs: inputs.into_iter().map(|v| v.into()).collect(),
                    outputs: outputs.into_iter().map(|v| v.into()).collect(),
                    witnesses: witnesses.into_iter().map(|v| v.into()).collect(),
                    metadata: metadata.map(|v| v.into()),
                },
                TransactionEnum::Create(TransactionCreate {
                    gas_price,
                    gas_limit,
                    byte_price,
                    maturity,
                    bytecode_witness_index,
                    salt,
                    static_contracts,
                    storage_slots,
                    inputs,
                    outputs,
                    witnesses,
                    metadata,
                }) => FuelTransaction::Create {
                    gas_price,
                    gas_limit,
                    byte_price,
                    maturity,
                    bytecode_witness_index,
                    salt: salt.into(),
                    static_contracts: static_contracts.into_iter().map(|v| v.into()).collect(),
                    storage_slots: storage_slots.into_iter().map(|v| v.into()).collect(),
                    inputs: inputs.into_iter().map(|v| v.into()).collect(),
                    outputs: outputs.into_iter().map(|v| v.into()).collect(),
                    witnesses: witnesses.into_iter().map(|v| v.into()).collect(),
                    metadata: metadata.map(|v| v.into()),
                },
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum TransactionEnum {
        Script(TransactionScript),
        Create(TransactionCreate),
    }

    #[derive(Debug, Clone, PartialEq, Eq, InputObject)]
    pub struct TransactionScript {
        gas_price: Word,
        gas_limit: Word,
        byte_price: Word,
        maturity: Word,
        receipts_root: Bytes32,
        script: Vec<u8>,
        script_data: Vec<u8>,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        witnesses: Vec<Vec<u8>>,
        metadata: Option<TransactionMetadata>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, InputObject)]
    pub struct TransactionCreate {
        gas_price: Word,
        gas_limit: Word,
        byte_price: Word,
        maturity: Word,
        bytecode_witness_index: u8,
        salt: Salt,
        static_contracts: Vec<ContractId>,
        storage_slots: Vec<StorageSlot>,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        witnesses: Vec<Vec<u8>>,
        metadata: Option<TransactionMetadata>,
    }
}
