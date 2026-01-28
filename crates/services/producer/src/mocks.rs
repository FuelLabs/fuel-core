use crate::ports::{
    BlockProducer,
    BlockProducerDatabase,
    DryRunner,
    Relayer,
    RelayerBlockInfo,
    TxPool,
};
use fuel_core_storage::{
    Result as StorageResult,
    not_found,
    transactional::{
        AtomicView,
        Changes,
    },
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            CompressedBlock,
        },
        header::{
            ConsensusParametersVersion,
            StateTransitionBytecodeVersion,
        },
        primitives::DaBlockHeight,
    },
    fuel_tx::Transaction,
    fuel_types::{
        BlockHeight,
        Bytes32,
        ChainId,
    },
    services::{
        block_producer::Components,
        executor::{
            DryRunResult,
            Error as ExecutorError,
            ExecutionResult,
            Result as ExecutorResult,
            UncommittedResult,
        },
    },
};
use std::{
    borrow::Cow,
    collections::HashMap,
    ops::Deref,
    sync::{
        Arc,
        Mutex,
    },
};
// Mockall-generated mocks for testing
#[cfg(feature = "test-helpers")]
use mockall::mock;

#[cfg(feature = "test-helpers")]
mock! {
    pub Relayer {}

    #[async_trait::async_trait]
    impl Relayer for Relayer {
        async fn wait_for_at_least_height(
            &self,
            height: &DaBlockHeight,
        ) -> anyhow::Result<DaBlockHeight>;

        async fn get_cost_and_transactions_number_for_block(
            &self,
            height: &DaBlockHeight,
        ) -> anyhow::Result<RelayerBlockInfo>;
    }
}

// Re-export for backward compatibility during migration
#[cfg(feature = "test-helpers")]
pub use MockRelayer as MockRelayerLegacy;

// Helper function to create a MockRelayer with default behavior
#[cfg(feature = "test-helpers")]
pub fn create_mock_relayer(
    latest_height: DaBlockHeight,
    blocks_with_costs: HashMap<DaBlockHeight, (u64, u64)>,
) -> MockRelayer {
    let mut mock = MockRelayer::new();

    // Default behavior: return the latest height
    mock.expect_wait_for_at_least_height()
        .returning(move |_| Ok(latest_height));

    // Default behavior: return gas cost and tx count from the map
    mock.expect_get_cost_and_transactions_number_for_block()
        .returning(move |height| {
            let (gas_cost, tx_count) =
                blocks_with_costs.get(height).cloned().unwrap_or_default();
            Ok(RelayerBlockInfo { gas_cost, tx_count })
        });

    mock
}

// MockTxPool with associated type
#[cfg(feature = "test-helpers")]
mock! {
    pub TxPool {}

    impl TxPool for TxPool {
        type TxSource = Vec<Transaction>;

        async fn get_source(&self, gas_price: u64, block_height: BlockHeight) -> anyhow::Result<Vec<Transaction>>;
    }
}

// Helper function to create a MockTxPool with default behavior
#[cfg(feature = "test-helpers")]
pub fn create_mock_txpool(transactions: Vec<Transaction>) -> MockTxPool {
    let mut mock = MockTxPool::new();

    // Default behavior: return the transactions
    mock.expect_get_source()
        .returning(move |_, _| Ok(transactions.clone()));

    mock
}

// MockExecutor - BlockProducer implementation
#[cfg(feature = "test-helpers")]
mock! {
    pub Executor {}

    impl BlockProducer<Vec<Transaction>> for Executor {
        type Deadline = ();

        async fn produce_without_commit(
            &self,
            component: Components<Vec<Transaction>>,
            deadline: (),
        ) -> ExecutorResult<UncommittedResult<Changes>>;
    }
}

// Helper function to create a MockExecutor with default successful behavior
#[cfg(feature = "test-helpers")]
pub fn create_mock_executor(db: MockDb) -> MockExecutor {
    let mut mock = MockExecutor::new();

    // Default behavior: create a block and insert it into the database
    mock.expect_produce_without_commit()
        .returning(move |component, _| {
            let block = arc_pool_tx_comp_to_block(&component);
            // simulate executor inserting a block
            let mut block_db = db.blocks.lock().unwrap();
            block_db.insert(
                *block.header().height(),
                block.compress(&ChainId::default()),
            );
            Ok(UncommittedResult::new(
                ExecutionResult {
                    block,
                    skipped_transactions: vec![],
                    tx_status: vec![],
                    events: vec![],
                },
                Default::default(),
            ))
        });

    mock
}

// Helper function to create a failing MockExecutor
// This replaces the old FailingMockExecutor struct
// Fails once with the provided error, then succeeds on subsequent calls
#[cfg(feature = "test-helpers")]
pub fn create_failing_mock_executor(error: ExecutorError, db: MockDb) -> MockExecutor {
    let mut mock = MockExecutor::new();

    // First call: fail with the provided error
    mock.expect_produce_without_commit()
        .times(1)
        .return_once(move |_, _| Err(error));

    // Subsequent calls: succeed with standard executor logic
    mock.expect_produce_without_commit()
        .returning(move |component, _| {
            let block = arc_pool_tx_comp_to_block(&component);
            let mut block_db = db.blocks.lock().unwrap();
            block_db.insert(
                *block.header().height(),
                block.compress(&ChainId::default()),
            );
            Ok(UncommittedResult::new(
                ExecutionResult {
                    block,
                    skipped_transactions: vec![],
                    tx_status: vec![],
                    events: vec![],
                },
                Default::default(),
            ))
        });

    mock
}

// Helper functions for block creation (used by mocks)
#[cfg(feature = "test-helpers")]
fn arc_pool_tx_comp_to_block(component: &Components<Vec<Transaction>>) -> Block {
    let transactions = component.transactions_source.clone();
    Block::new(
        component.header_to_produce,
        transactions,
        &[],
        Default::default(),
        #[cfg(feature = "fault-proving")]
        &Default::default(),
    )
    .unwrap()
}

impl AsMut<MockDb> for MockDb {
    fn as_mut(&mut self) -> &mut MockDb {
        self
    }
}

impl AsRef<MockDb> for MockDb {
    fn as_ref(&self) -> &MockDb {
        self
    }
}

#[derive(Clone)]
pub struct MockExecutorWithCapture {
    pub captured: Arc<Mutex<Option<Components<Vec<Transaction>>>>>,
}

impl BlockProducer<Vec<Transaction>> for MockExecutorWithCapture {
    type Deadline = ();
    async fn produce_without_commit(
        &self,
        component: Components<Vec<Transaction>>,
        _: (),
    ) -> ExecutorResult<UncommittedResult<Changes>> {
        let block = arc_pool_tx_comp_to_block(&component);
        *self.captured.lock().unwrap() = Some(component);
        Ok(UncommittedResult::new(
            ExecutionResult {
                block,
                skipped_transactions: vec![],
                tx_status: vec![],
                events: vec![],
            },
            Default::default(),
        ))
    }
}

impl DryRunner for MockExecutorWithCapture {
    fn dry_run(
        &self,
        block: Components<Vec<Transaction>>,
        _utxo_validation: Option<bool>,
        _height: Option<BlockHeight>,
        _record_storage_read_replay: bool,
    ) -> ExecutorResult<DryRunResult> {
        *self.captured.lock().unwrap() = Some(block);

        Ok(DryRunResult {
            transactions: Vec::new(),
            storage_reads: Vec::new(),
        })
    }
}

impl Default for MockExecutorWithCapture {
    fn default() -> Self {
        Self {
            captured: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct MockDb {
    pub blocks: Arc<Mutex<HashMap<BlockHeight, CompressedBlock>>>,
    pub consensus_parameters_version: ConsensusParametersVersion,
    pub state_transition_bytecode_version: StateTransitionBytecodeVersion,
}

impl AtomicView for MockDb {
    type LatestView = Self;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(self.clone())
    }
}

impl BlockProducerDatabase for MockDb {
    fn latest_height(&self) -> Option<BlockHeight> {
        let blocks = self.blocks.lock().unwrap();

        blocks.keys().max().cloned()
    }

    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<'_, CompressedBlock>> {
        let blocks = self.blocks.lock().unwrap();
        blocks
            .get(height)
            .cloned()
            .map(Cow::Owned)
            .ok_or(not_found!("Didn't find block for test"))
    }

    fn get_full_block(&self, _height: &BlockHeight) -> StorageResult<Block> {
        unimplemented!("Not used by tests");
    }

    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32> {
        Ok(Bytes32::new(
            [u8::try_from(*height.deref()).expect("Test use small values"); 32],
        ))
    }

    fn latest_consensus_parameters_version(
        &self,
    ) -> StorageResult<ConsensusParametersVersion> {
        Ok(self.consensus_parameters_version)
    }

    fn latest_state_transition_bytecode_version(
        &self,
    ) -> StorageResult<StateTransitionBytecodeVersion> {
        Ok(self.state_transition_bytecode_version)
    }
}
