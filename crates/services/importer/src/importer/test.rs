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
        Transaction,
    },
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        primitives::{
            BlockHeight,
            BlockId,
        },
        SealedBlock,
    },
    fuel_types::Bytes32,
    services::{
        block_importer::{
            ImportResult,
            UncommittedResult,
        },
        executor::{
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

        fn insert_block_header_merkle_root(
            &mut self,
            height: &BlockHeight,
            root: &Bytes32,
        ) -> StorageResult<Option<Bytes32>>;
    }

    impl Transaction<MockDatabase> for Database {
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

fn executor_db<H, S, R>(
    height: H,
    seal: S,
    root: R,
    commit: usize,
) -> impl Fn() -> MockDatabase
where
    H: Fn() -> StorageResult<u32> + Send + Clone + 'static,
    S: Fn() -> StorageResult<Option<Consensus>> + Send + Clone + 'static,
    R: Fn() -> StorageResult<Option<Bytes32>> + Send + Clone + 'static,
{
    move || {
        let height = height.clone();
        let seal = seal.clone();
        let root = root.clone();
        let mut db = MockDatabase::default();
        db.expect_latest_block_height()
            .returning(move || height().map(Into::into));
        db.expect_seal_block().returning(move |_, _| seal());
        db.expect_insert_block_header_merkle_root()
            .returning(move |_, _| root());
        db.expect_commit().times(commit).returning(|| Ok(()));

        db
    }
}

fn ok<T: Clone, Err>(entity: T) -> impl Fn() -> Result<T, Err> + Clone {
    move || Ok(entity.clone())
}

fn not_found<T>() -> StorageResult<T> {
    Err(not_found!("Not found"))
}

fn failure<T>() -> StorageResult<T> {
    Err(StorageError::Other(anyhow!("Some failure")))
}

fn storage_failure_error() -> Error {
    Error::StorageError(StorageError::Other(anyhow!("Some failure")))
}

fn executor<B>(block: B, database: MockDatabase) -> MockExecutor
where
    B: Fn() -> ExecutorResult<Block> + Send + 'static,
{
    let mut executor = MockExecutor::default();
    executor
        .expect_execute_without_commit()
        .return_once(move |_| {
            Ok(Uncommitted::new(
                ExecutionResult {
                    block: block()?,
                    skipped_transactions: vec![],
                    tx_status: vec![],
                },
                StorageTransaction::new(database),
            ))
        });

    executor
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
    executor_db(ok(0), ok(None), ok(None), 1)
    => Ok(())
)]
#[test_case(
    genesis(113),
    underlying_db(not_found),
    executor_db(ok(113), ok(None), ok(None), 1)
    => Ok(())
)]
#[test_case(
    genesis(0),
    underlying_db(failure),
    executor_db(ok(0), ok(None), ok(None), 0)
    => Err(Error::InvalidUnderlyingDatabaseGenesisState)
)]
#[test_case(
    genesis(0),
    underlying_db(ok(0)),
    executor_db(ok(0), ok(None), ok(None), 0)
    => Err(Error::InvalidUnderlyingDatabaseGenesisState)
)]
#[test_case(
    genesis(1),
    underlying_db(not_found),
    executor_db(ok(0), ok(None), ok(None), 0)
    => Err(Error::InvalidDatabaseStateAfterExecution(1u32.into(), 0u32.into()))
)]
#[test_case(
    genesis(0),
    underlying_db(not_found),
    executor_db(ok(0), ok(Some(Default::default())), ok(None), 0)
    => Err(Error::NotUnique(0u32.into()))
)]
#[test_case(
    genesis(0),
    underlying_db(not_found),
    executor_db(ok(0), ok(None), ok(Some(Default::default())), 0)
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
    executor_db(ok(1), ok(None), ok(None), 1)
    => Ok(())
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(113), ok(None), ok(None), 1)
    => Ok(())
)]
#[test_case(
    poa_block(0),
    underlying_db(ok(0)),
    executor_db(ok(1), ok(None), ok(None), 0)
    => Err(Error::ZeroNonGenericHeight)
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(111)),
    executor_db(ok(113), ok(None), ok(None), 0)
    => Err(Error::IncorrectBlockHeight(112u32.into(), 113u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(114)),
    executor_db(ok(113), ok(None), ok(None), 0)
    => Err(Error::IncorrectBlockHeight(115u32.into(), 113u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(114), ok(None), ok(None), 0)
    => Err(Error::InvalidDatabaseStateAfterExecution(113u32.into(), 114u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(failure, ok(None), ok(None), 0)
    => Err(storage_failure_error())
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(113), ok(Some(Default::default())), ok(None), 0)
    => Err(Error::NotUnique(113u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(113), ok(None), ok(Some(Default::default())), 0)
    => Err(Error::NotUnique(113u32.into()))
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(113), failure, ok(None), 0)
    => Err(storage_failure_error())
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(112)),
    executor_db(ok(113), ok(None), failure, 0)
    => Err(storage_failure_error())
)]
fn commit_result_poa(
    sealed_block: SealedBlock,
    underlying_db: impl Fn() -> MockDatabase,
    executor_db: impl Fn() -> MockDatabase,
) -> Result<(), Error> {
    // `execute_and_commit` and `commit_result` should have the same
    // validation rules(-> test cases) during committing the result.
    let commit_result =
        commit_result_assert(sealed_block.clone(), underlying_db(), executor_db());
    let execute_and_commit_result = execute_and_commit_assert(
        sealed_block.clone(),
        underlying_db(),
        executor(ok(sealed_block.entity), executor_db()),
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
