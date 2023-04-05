use crate::{
    importer::Error,
    ports::{
        ExecutorDatabase,
        ImporterDatabase,
        MockBlockVerifier,
        MockExecutor,
    },
    Importer,
};
use anyhow::anyhow;
use fuel_core_storage::{
    not_found,
    transactional::{
        StorageTransaction,
        Transaction as TransactionTrait,
    },
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        primitives::BlockId,
        SealedBlock,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::{
        block_importer::{
            ImportResult,
            UncommittedResult,
        },
        executor::{
            Error as ExecutorError,
            ExecutionResult,
            Result as ExecutorResult,
        },
        Uncommitted,
    },
};
use test_case::test_case;
use tokio::sync::{
    broadcast::error::TryRecvError,
    TryAcquireError,
};

mockall::mock! {
    pub Database {}

    impl ImporterDatabase for Database {
        fn latest_block_height(&self) -> StorageResult<BlockHeight>;
    }

    impl ExecutorDatabase for Database {
        fn seal_block(
            &mut self,
            block_id: &BlockId,
            consensus: &Consensus,
        ) -> StorageResult<Option<Consensus>>;
    }

    impl TransactionTrait<MockDatabase> for Database {
        fn commit(&mut self) -> StorageResult<()>;
    }
}

impl AsMut<MockDatabase> for MockDatabase {
    fn as_mut(&mut self) -> &mut MockDatabase {
        self
    }
}

impl AsRef<MockDatabase> for MockDatabase {
    fn as_ref(&self) -> &MockDatabase {
        self
    }
}

#[derive(Clone, Debug)]
struct MockExecutionResult {
    block: SealedBlock,
    skipped_transactions: usize,
}

fn genesis(height: u32) -> SealedBlock {
    let mut block = Block::default();
    block.header_mut().consensus.height = height.into();
    block.header_mut().recalculate_metadata();

    SealedBlock {
        entity: block,
        consensus: Consensus::Genesis(Default::default()),
    }
}

fn poa_block(height: u32) -> SealedBlock {
    let mut block = Block::default();
    block.header_mut().consensus.height = height.into();
    block.header_mut().recalculate_metadata();

    SealedBlock {
        entity: block,
        consensus: Consensus::PoA(Default::default()),
    }
}

fn underlying_db<R>(result: R) -> impl Fn() -> MockDatabase
where
    R: Fn() -> StorageResult<u32> + Send + Clone + 'static,
{
    move || {
        let result = result.clone();
        let mut db = MockDatabase::default();
        db.expect_latest_block_height()
            .returning(move || result().map(Into::into));
        db
    }
}

fn executor_db<H, S>(height: H, seal: S, commits: usize) -> impl Fn() -> MockDatabase
where
    H: Fn() -> StorageResult<u32> + Send + Clone + 'static,
    S: Fn() -> StorageResult<Option<Consensus>> + Send + Clone + 'static,
{
    move || {
        let height = height.clone();
        let seal = seal.clone();
        let mut db = MockDatabase::default();
        db.expect_latest_block_height()
            .returning(move || height().map(Into::into));
        db.expect_seal_block().returning(move |_, _| seal());
        db.expect_commit().times(commits).returning(|| Ok(()));

        db
    }
}

fn ok<T: Clone, Err>(entity: T) -> impl Fn() -> Result<T, Err> + Clone {
    move || Ok(entity.clone())
}

fn not_found<T>() -> StorageResult<T> {
    Err(not_found!("Not found"))
}

fn storage_failure<T>() -> StorageResult<T> {
    Err(StorageError::Other(anyhow!("Some failure")))
}

fn storage_failure_error() -> Error {
    Error::StorageError(StorageError::Other(anyhow!("Some failure")))
}

fn ex_result(height: u32, skipped_transactions: usize) -> MockExecutionResult {
    MockExecutionResult {
        block: poa_block(height),
        skipped_transactions,
    }
}

fn execution_failure<T>() -> ExecutorResult<T> {
    Err(ExecutorError::InvalidBlockId)
}

fn execution_failure_error() -> Error {
    Error::FailedExecution(ExecutorError::InvalidBlockId)
}

fn executor<R>(result: R, database: MockDatabase) -> MockExecutor
where
    R: Fn() -> ExecutorResult<MockExecutionResult> + Send + 'static,
{
    let mut executor = MockExecutor::default();
    executor
        .expect_execute_without_commit()
        .return_once(move |_| {
            let mock_result = result()?;
            let skipped_transactions: Vec<_> = (0..mock_result.skipped_transactions)
                .map(|_| (Transaction::default(), ExecutorError::InvalidBlockId))
                .collect();
            Ok(Uncommitted::new(
                ExecutionResult {
                    block: mock_result.block.entity,
                    skipped_transactions,
                    tx_status: vec![],
                },
                StorageTransaction::new(database),
            ))
        });

    executor
}

fn verification_failure<T>() -> anyhow::Result<T> {
    Err(anyhow!("Not verified"))
}

fn verification_failure_error() -> Error {
    Error::FailedVerification(anyhow!("Not verified"))
}

fn verifier<R>(result: R) -> MockBlockVerifier
where
    R: Fn() -> anyhow::Result<()> + Send + 'static,
{
    let mut verifier = MockBlockVerifier::default();
    verifier
        .expect_verify_block_fields()
        .return_once(move |_, _| result());

    verifier
}

//////////////// SealedBlock, UnderlyingDB, ExecutionDB ///////////////
//////////////// //////////// Genesis Block /////////// ////////////////
#[test_case(
    genesis(0),
    underlying_db(not_found),
    executor_db(ok(0), ok(None), 1)
    => Ok(())
)]
#[test_case(
    genesis(113),
    underlying_db(not_found),
    executor_db(ok(113), ok(None), 1)
    => Ok(())
)]
#[test_case(
    genesis(0),
    underlying_db(storage_failure),
    executor_db(ok(0), ok(None), 0)
    => Err(Error::InvalidUnderlyingDatabaseGenesisState)
)]
#[test_case(
    genesis(0),
    underlying_db(ok(0)),
    executor_db(ok(0), ok(None), 0)
    => Err(Error::InvalidUnderlyingDatabaseGenesisState)
)]
#[test_case(
    genesis(1),
    underlying_db(not_found),
    executor_db(ok(0), ok(None), 0)
    => Err(Error::InvalidDatabaseStateAfterExecution(1u32.into(), 0u32.into()))
)]
#[test_case(
    genesis(0),
    underlying_db(not_found),
    executor_db(ok(0), ok(Some(Default::default())), 0)
    => Err(Error::NotUnique(0u32.into()))
)]
fn commit_result_genesis(
    sealed_block: SealedBlock,
    underlying_db: impl Fn() -> MockDatabase,
    executor_db: impl Fn() -> MockDatabase,
) -> Result<(), Error> {
    commit_result_assert(sealed_block, underlying_db(), executor_db())
}

//////////////////////////// PoA Block ////////////////////////////
#[test_case(
    poa_block(1),
    underlying_db(ok(0)),
    executor_db(ok(1), ok(None), 1)
    => Ok(())
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(113), ok(None), 1)
    => Ok(())
)]
#[test_case(
    poa_block(0),
    underlying_db(ok(0)),
    executor_db(ok(1), ok(None), 0)
    => Err(Error::ZeroNonGenericHeight)
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(111)),
    executor_db(ok(113), ok(None), 0)
    => Err(Error::IncorrectBlockHeight(112u32.into(), 113u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(114)),
    executor_db(ok(113), ok(None), 0)
    => Err(Error::IncorrectBlockHeight(115u32.into(), 113u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(114), ok(None), 0)
    => Err(Error::InvalidDatabaseStateAfterExecution(113u32.into(), 114u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(storage_failure, ok(None), 0)
    => Err(storage_failure_error())
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(113), ok(Some(Default::default())), 0)
    => Err(Error::NotUnique(113u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(113), storage_failure, 0)
    => Err(storage_failure_error())
)]
fn commit_result_and_execute_and_commit_poa(
    sealed_block: SealedBlock,
    underlying_db: impl Fn() -> MockDatabase,
    executor_db: impl Fn() -> MockDatabase,
) -> Result<(), Error> {
    // `execute_and_commit` and `commit_result` should have the same
    // validation rules(-> test cases) during committing the result.
    let height = *sealed_block.entity.header().height();
    let commit_result =
        commit_result_assert(sealed_block.clone(), underlying_db(), executor_db());
    let execute_and_commit_result = execute_and_commit_assert(
        sealed_block,
        underlying_db(),
        executor(ok(ex_result(height.into(), 0)), executor_db()),
        verifier(ok(())),
    );
    assert_eq!(commit_result, execute_and_commit_result);
    commit_result
}

fn commit_result_assert(
    sealed_block: SealedBlock,
    underlying_db: MockDatabase,
    executor_db: MockDatabase,
) -> Result<(), Error> {
    let expected_to_broadcast = sealed_block.clone();
    let importer = Importer::new(Default::default(), underlying_db, (), ());
    let uncommitted_result = UncommittedResult::new(
        ImportResult {
            sealed_block,
            tx_status: vec![],
        },
        StorageTransaction::new(executor_db),
    );

    let mut imported_blocks = importer.subscribe();
    let result = importer.commit_result(uncommitted_result);

    if result.is_ok() {
        let actual_sealed_block = imported_blocks.try_recv().unwrap();
        assert_eq!(actual_sealed_block.sealed_block, expected_to_broadcast);
        assert_eq!(
            imported_blocks
                .try_recv()
                .expect_err("We should broadcast only one block"),
            TryRecvError::Empty
        )
    }

    result
}

fn execute_and_commit_assert(
    sealed_block: SealedBlock,
    underlying_db: MockDatabase,
    executor: MockExecutor,
    verifier: MockBlockVerifier,
) -> Result<(), Error> {
    let expected_to_broadcast = sealed_block.clone();
    let importer = Importer::new(Default::default(), underlying_db, executor, verifier);

    let mut imported_blocks = importer.subscribe();
    let result = importer.execute_and_commit(sealed_block);

    if result.is_ok() {
        let actual_sealed_block = imported_blocks.try_recv().unwrap();
        assert_eq!(actual_sealed_block.sealed_block, expected_to_broadcast);
        assert_eq!(
            imported_blocks
                .try_recv()
                .expect_err("We should broadcast only one block"),
            TryRecvError::Empty
        )
    }

    result
}

#[test]
fn commit_result_fail_when_locked() {
    let importer = Importer::new(Default::default(), MockDatabase::default(), (), ());
    let uncommitted_result = UncommittedResult::new(
        ImportResult {
            sealed_block: Default::default(),
            tx_status: vec![],
        },
        StorageTransaction::new(MockDatabase::default()),
    );

    let _guard = importer.lock();
    assert_eq!(
        importer.commit_result(uncommitted_result),
        Err(Error::SemaphoreError(TryAcquireError::NoPermits))
    );
}

#[test]
fn execute_and_commit_fail_when_locked() {
    let importer = Importer::new(
        Default::default(),
        MockDatabase::default(),
        MockExecutor::default(),
        MockBlockVerifier::default(),
    );

    let _guard = importer.lock();
    assert_eq!(
        importer.execute_and_commit(Default::default()),
        Err(Error::SemaphoreError(TryAcquireError::NoPermits))
    );
}

#[test]
fn one_lock_at_the_same_time() {
    let importer = Importer::new(
        Default::default(),
        MockDatabase::default(),
        MockExecutor::default(),
        MockBlockVerifier::default(),
    );

    let _guard = importer.lock();
    assert_eq!(
        importer.lock().map(|_| ()),
        Err(Error::SemaphoreError(TryAcquireError::NoPermits))
    );
}

///////// New block, Block After Execution, Verification result, commits /////////
#[test_case(
    genesis(113), ok(ex_result(113, 0)), ok(()), 0
    => Err(Error::ExecuteGenesis)
)]
#[test_case(
    poa_block(1), ok(ex_result(1, 0)), ok(()), 1
    => Ok(())
)]
#[test_case(
    poa_block(113), ok(ex_result(113, 0)), ok(()), 1
    => Ok(())
)]
#[test_case(
    poa_block(113), ok(ex_result(114, 0)), ok(()), 0
    => Err(Error::BlockIdMismatch(poa_block(113).entity.id(), poa_block(114).entity.id()))
)]
#[test_case(
    poa_block(113), execution_failure, ok(()), 0
    => Err(execution_failure_error())
)]
#[test_case(
    poa_block(113), ok(ex_result(113, 1)), ok(()), 0
    => Err(Error::SkippedTransactionsNotEmpty)
)]
#[test_case(
    poa_block(113), ok(ex_result(113, 0)), verification_failure, 0
    => Err(verification_failure_error())
)]
fn execute_and_commit_and_verify_and_execute_block_poa<V, P>(
    sealed_block: SealedBlock,
    block_after_execution: P,
    verifier_result: V,
    commits: usize,
) -> Result<(), Error>
where
    P: Fn() -> ExecutorResult<MockExecutionResult> + Send + Clone + 'static,
    V: Fn() -> anyhow::Result<()> + Send + Clone + 'static,
{
    // `execute_and_commit` and `verify_and_execute_block` should have the same
    // validation rules(-> test cases) during verification.
    let verify_and_execute_result = verify_and_execute_assert(
        sealed_block.clone(),
        block_after_execution.clone(),
        verifier_result.clone(),
    );

    // We tested commit part in the `commit_result_and_execute_and_commit_poa` so setup the
    // databases to always pass the committing part.
    let expected_height: u32 = sealed_block.entity.header().consensus.height.into();
    let previous_height = expected_height.checked_sub(1).unwrap_or_default();
    let execute_and_commit_result = execute_and_commit_assert(
        sealed_block,
        underlying_db(ok(previous_height))(),
        executor(
            block_after_execution,
            executor_db(ok(expected_height), ok(None), commits)(),
        ),
        verifier(verifier_result),
    );
    assert_eq!(verify_and_execute_result, execute_and_commit_result);
    execute_and_commit_result
}

fn verify_and_execute_assert<P, V>(
    sealed_block: SealedBlock,
    block_after_execution: P,
    verifier_result: V,
) -> Result<(), Error>
where
    P: Fn() -> ExecutorResult<MockExecutionResult> + Send + 'static,
    V: Fn() -> anyhow::Result<()> + Send + 'static,
{
    let importer = Importer::new(
        Default::default(),
        MockDatabase::default(),
        executor(block_after_execution, MockDatabase::default()),
        verifier(verifier_result),
    );

    importer.verify_and_execute_block(sealed_block).map(|_| ())
}

#[test]
fn verify_and_execute_allowed_when_locked() {
    let importer = Importer::new(
        Default::default(),
        MockDatabase::default(),
        executor(ok(ex_result(13, 0)), MockDatabase::default()),
        verifier(ok(())),
    );

    let _guard = importer.lock();
    assert!(importer.verify_and_execute_block(poa_block(13)).is_ok());
}
