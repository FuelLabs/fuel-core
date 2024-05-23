use crate::{
    importer::Error,
    ports::{
        ImporterDatabase,
        MockBlockVerifier,
        MockDatabaseTransaction,
        MockValidator,
        Transactional,
    },
    Importer,
};
use anyhow::anyhow;
use fuel_core_storage::{
    transactional::Changes,
    Error as StorageError,
    MerkleRoot,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        SealedBlock,
    },
    fuel_types::BlockHeight,
    services::{
        block_importer::{
            ImportResult,
            UncommittedResult,
        },
        executor::{
            Error as ExecutorError,
            Result as ExecutorResult,
            UncommittedValidationResult,
            ValidationResult,
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

    impl Transactional for Database {
        type Transaction<'a> = MockDatabaseTransaction
        where
            Self: 'a;

        fn storage_transaction(&mut self, changes: Changes) -> MockDatabaseTransaction;
    }

    impl ImporterDatabase for Database {
        fn latest_block_height(&self) -> StorageResult<Option<BlockHeight>>;

        fn latest_block_root(&self) -> StorageResult<Option<MerkleRoot>>;
    }
}

fn u32_to_merkle_root(number: u32) -> MerkleRoot {
    let mut root = [0; 32];
    root[0..4].copy_from_slice(&number.to_be_bytes());
    MerkleRoot::from(root)
}

fn genesis(height: u32) -> SealedBlock {
    let mut block = Block::default();
    block.header_mut().set_block_height(height.into());
    block.header_mut().recalculate_metadata();

    SealedBlock {
        entity: block,
        consensus: Consensus::Genesis(Default::default()),
    }
}

fn poa_block(height: u32) -> SealedBlock {
    let mut block = Block::default();
    block.header_mut().set_block_height(height.into());
    block.header_mut().recalculate_metadata();

    SealedBlock {
        entity: block,
        consensus: Consensus::PoA(Default::default()),
    }
}

fn underlying_db<R>(result: R) -> impl Fn() -> MockDatabase
where
    R: Fn() -> StorageResult<Option<u32>> + Send + Clone + 'static,
{
    move || {
        let result_height = result.clone();
        let result_root = result.clone();
        let mut db = MockDatabase::default();
        db.expect_latest_block_height()
            .returning(move || result_height().map(|v| v.map(Into::into)));
        db.expect_latest_block_root()
            .returning(move || result_root().map(|v| v.map(u32_to_merkle_root)));
        db
    }
}

fn db_transaction<H, B>(
    height: H,
    store_block: B,
    commits: usize,
) -> impl Fn() -> MockDatabaseTransaction
where
    H: Fn() -> StorageResult<Option<u32>> + Send + Clone + 'static,
    B: Fn() -> StorageResult<bool> + Send + Clone + 'static,
{
    move || {
        let height = height.clone();
        let store_block = store_block.clone();
        let mut db = MockDatabaseTransaction::default();
        db.expect_latest_block_root()
            .returning(move || height().map(|v| v.map(u32_to_merkle_root)));
        db.expect_store_new_block()
            .returning(move |_, _| store_block());
        db.expect_commit().times(commits).returning(|| Ok(()));
        db
    }
}

fn ok<T: Clone, Err>(entity: T) -> impl Fn() -> Result<T, Err> + Clone {
    move || Ok(entity.clone())
}

fn storage_failure<T>() -> StorageResult<T> {
    Err(StorageError::Other(anyhow!("Some failure")))
}

fn storage_failure_error() -> Error {
    storage_failure::<()>().unwrap_err().into()
}

fn ex_result() -> ExecutorResult<UncommittedValidationResult<Changes>> {
    Ok(Uncommitted::new(
        ValidationResult {
            tx_status: vec![],
            events: vec![],
        },
        Default::default(),
    ))
}

fn execution_failure<T>() -> ExecutorResult<T> {
    Err(ExecutorError::BlockMismatch)
}

fn execution_failure_error() -> Error {
    Error::FailedExecution(ExecutorError::BlockMismatch)
}

fn executor<R>(result: R) -> MockValidator
where
    R: Fn() -> ExecutorResult<UncommittedValidationResult<Changes>> + Send + 'static,
{
    let mut executor = MockValidator::default();
    executor.expect_validate().return_once(move |_| result());

    executor
}

fn verification_failure<T>() -> anyhow::Result<T> {
    Err(anyhow!("Not verified"))
}

fn verification_failure_error() -> Error {
    Error::FailedVerification(verification_failure::<()>().unwrap_err())
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
    underlying_db(ok(None)),
    db_transaction(ok(None), ok(true), 1)
    => Ok(());
    "successfully imports genesis block when latest block not found"
)]
#[test_case(
    genesis(113),
    underlying_db(ok(None)),
    db_transaction(ok(None), ok(true), 1)
    => Ok(());
    "successfully imports block at arbitrary height when executor db expects it and last block not found" 
)]
#[test_case(
    genesis(0),
    underlying_db(storage_failure),
    db_transaction(ok(Some(0)), ok(true), 0)
    => Err(storage_failure_error());
    "fails to import genesis when underlying database fails"
)]
#[test_case(
    genesis(0),
    underlying_db(ok(Some(0))),
    db_transaction(ok(Some(0)), ok(true), 0)
    => Err(Error::InvalidUnderlyingDatabaseGenesisState);
    "fails to import genesis block when already exists"
)]
#[test_case(
    genesis(1),
    underlying_db(ok(None)),
    db_transaction(ok(Some(0)), ok(true), 0)
    => Err(Error::InvalidDatabaseStateAfterExecution(None, Some(u32_to_merkle_root(0))));
    "fails to import genesis block when next height is not 0"
)]
#[test_case(
    genesis(0),
    underlying_db(ok(None)),
    db_transaction(ok(None), ok(false), 0)
    => Err(Error::NotUnique(0u32.into()));
    "fails to import genesis block when block exists for height 0"
)]
#[tokio::test]
async fn commit_result_genesis(
    sealed_block: SealedBlock,
    underlying_db: impl Fn() -> MockDatabase,
    db_transaction: impl Fn() -> MockDatabaseTransaction,
) -> Result<(), Error> {
    commit_result_assert(sealed_block, underlying_db(), db_transaction()).await
}

//////////////////////////// PoA Block ////////////////////////////
#[test_case(
    poa_block(1),
    underlying_db(ok(Some(0))),
    db_transaction(ok(Some(0)), ok(true), 1)
    => Ok(());
    "successfully imports block at height 1 when latest block is genesis"
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(Some(112))),
    db_transaction(ok(Some(112)), ok(true), 1)
    => Ok(());
    "successfully imports block at arbitrary height when latest block height is one fewer and executor db expects it"
)]
#[test_case(
    poa_block(0),
    underlying_db(ok(Some(0))),
    db_transaction(ok(Some(1)), ok(true), 0)
    => Err(Error::ZeroNonGenericHeight);
    "fails to import PoA block with height 0"
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(Some(111))),
    db_transaction(ok(Some(113)), ok(true), 0)
    => Err(Error::IncorrectBlockHeight(112u32.into(), 113u32.into()));
    "fails to import block at height 113 when latest block height is 111"
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(Some(114))),
    db_transaction(ok(Some(113)), ok(true), 0)
    => Err(Error::IncorrectBlockHeight(115u32.into(), 113u32.into()));
    "fails to import block at height 113 when latest block height is 114"
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(Some(112))),
    db_transaction(ok(Some(114)), ok(true), 0)
    => Err(Error::InvalidDatabaseStateAfterExecution(Some(u32_to_merkle_root(112u32)), Some(u32_to_merkle_root(114u32))));
    "fails to import block 113 when executor db expects height 114"
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(Some(112))),
    db_transaction(storage_failure, ok(true), 0)
    => Err(storage_failure_error());
    "fails to import block when executor db fails to find latest block"
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(Some(112))),
    db_transaction(ok(Some(112)), ok(false), 0)
    => Err(Error::NotUnique(113u32.into()));
    "fails to import block when block exists"
)]
#[test_case(
    poa_block(113),
    underlying_db(ok(Some(112))),
    db_transaction(ok(Some(112)), storage_failure, 0)
    => Err(storage_failure_error());
    "fails to import block when executor db fails to find block"
)]
#[tokio::test]
async fn commit_result_and_execute_and_commit_poa(
    sealed_block: SealedBlock,
    underlying_db: impl Fn() -> MockDatabase,
    db_transaction: impl Fn() -> MockDatabaseTransaction,
) -> Result<(), Error> {
    // `execute_and_commit` and `commit_result` should have the same
    // validation rules(-> test cases) during committing the result.
    let transaction = db_transaction();
    let commit_result =
        commit_result_assert(sealed_block.clone(), underlying_db(), transaction).await;
    let transaction = db_transaction();
    let mut db = underlying_db();
    db.expect_storage_transaction().return_once(|_| transaction);
    let execute_and_commit_result = execute_and_commit_assert(
        sealed_block,
        db,
        executor(ex_result),
        verifier(ok(())),
    )
    .await;
    assert_eq!(commit_result, execute_and_commit_result);
    commit_result
}

async fn commit_result_assert(
    sealed_block: SealedBlock,
    mut underlying_db: MockDatabase,
    db_transaction: MockDatabaseTransaction,
) -> Result<(), Error> {
    underlying_db
        .expect_storage_transaction()
        .return_once(|_| db_transaction);
    let expected_to_broadcast = sealed_block.clone();
    let importer = Importer::default_config(underlying_db, (), ());
    let uncommitted_result = UncommittedResult::new(
        ImportResult::new_from_local(sealed_block, vec![], vec![]),
        Default::default(),
    );

    let mut imported_blocks = importer.subscribe();
    let result = importer.commit_result(uncommitted_result).await;

    if result.is_ok() {
        let actual_sealed_block = imported_blocks.try_recv().unwrap();
        assert_eq!(actual_sealed_block.sealed_block, expected_to_broadcast);
        if let Err(err) = imported_blocks.try_recv() {
            assert_eq!(err, TryRecvError::Empty);
        } else {
            panic!("We should broadcast only one block");
        }
    }

    result
}

async fn execute_and_commit_assert(
    sealed_block: SealedBlock,
    underlying_db: MockDatabase,
    executor: MockValidator,
    verifier: MockBlockVerifier,
) -> Result<(), Error> {
    let expected_to_broadcast = sealed_block.clone();
    let importer = Importer::default_config(underlying_db, executor, verifier);

    let mut imported_blocks = importer.subscribe();
    let result = importer.execute_and_commit(sealed_block).await;

    if result.is_ok() {
        let actual_sealed_block = imported_blocks.try_recv().unwrap();
        assert_eq!(actual_sealed_block.sealed_block, expected_to_broadcast);

        if let Err(err) = imported_blocks.try_recv() {
            assert_eq!(err, TryRecvError::Empty);
        } else {
            panic!("We should broadcast only one block");
        }
    }

    result
}

#[tokio::test]
async fn commit_result_fail_when_locked() {
    let importer = Importer::default_config(MockDatabase::default(), (), ());
    let uncommitted_result =
        UncommittedResult::new(ImportResult::default(), Default::default());

    let _guard = importer.lock();
    assert_eq!(
        importer.commit_result(uncommitted_result).await,
        Err(Error::SemaphoreError(TryAcquireError::NoPermits))
    );
}

#[tokio::test]
async fn execute_and_commit_fail_when_locked() {
    let importer = Importer::default_config(
        MockDatabase::default(),
        MockValidator::default(),
        MockBlockVerifier::default(),
    );

    let _guard = importer.lock();
    assert_eq!(
        importer.execute_and_commit(Default::default()).await,
        Err(Error::SemaphoreError(TryAcquireError::NoPermits))
    );
}

#[test]
fn one_lock_at_the_same_time() {
    let importer = Importer::default_config(
        MockDatabase::default(),
        MockValidator::default(),
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
    genesis(113), ex_result, ok(()), 0
    => Err(Error::ExecuteGenesis);
    "cannot execute genesis block"
)]
#[test_case(
    poa_block(1), ex_result, ok(()), 1
    => Ok(());
    "commits block 1"
)]
#[test_case(
    poa_block(113), ex_result, ok(()), 1
    => Ok(());
    "commits block 113"
)]
#[test_case(
    poa_block(113), execution_failure, ok(()), 0
    => Err(execution_failure_error());
    "commit fails if execution fails"
)]
#[test_case(
    poa_block(113), ex_result, verification_failure, 0
    => Err(verification_failure_error());
    "commit fails if verification fails"
)]
#[tokio::test]
async fn execute_and_commit_and_verify_and_execute_block_poa<V, P>(
    sealed_block: SealedBlock,
    block_after_execution: P,
    verifier_result: V,
    commits: usize,
) -> Result<(), Error>
where
    P: Fn() -> ExecutorResult<UncommittedValidationResult<Changes>>
        + Send
        + Clone
        + 'static,
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
    let expected_height: u32 = (*sealed_block.entity.header().height()).into();
    let previous_height = expected_height.checked_sub(1).unwrap_or_default();
    let mut db = underlying_db(ok(Some(previous_height)))();
    db.expect_storage_transaction().return_once(move |_| {
        db_transaction(ok(Some(previous_height)), ok(true), commits)()
    });
    let execute_and_commit_result = execute_and_commit_assert(
        sealed_block,
        db,
        executor(block_after_execution),
        verifier(verifier_result),
    )
    .await;
    assert_eq!(verify_and_execute_result, execute_and_commit_result);
    execute_and_commit_result
}

fn verify_and_execute_assert<P, V>(
    sealed_block: SealedBlock,
    block_after_execution: P,
    verifier_result: V,
) -> Result<(), Error>
where
    P: Fn() -> ExecutorResult<UncommittedValidationResult<Changes>> + Send + 'static,
    V: Fn() -> anyhow::Result<()> + Send + 'static,
{
    let importer = Importer::default_config(
        MockDatabase::default(),
        executor(block_after_execution),
        verifier(verifier_result),
    );

    importer.verify_and_execute_block(sealed_block).map(|_| ())
}

#[test]
fn verify_and_execute_allowed_when_locked() {
    let importer = Importer::default_config(
        MockDatabase::default(),
        executor(ex_result),
        verifier(ok(())),
    );

    let _guard = importer.lock();
    assert!(importer.verify_and_execute_block(poa_block(13)).is_ok());
}
