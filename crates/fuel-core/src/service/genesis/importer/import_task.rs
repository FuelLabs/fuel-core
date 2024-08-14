use anyhow::bail;
use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    structured_storage::TableWithBlueprint,
    transactional::{
        Modifiable,
        StorageTransaction,
        WriteTransaction,
    },
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};

use crate::{
    database::{
        database_description::DatabaseDescription,
        genesis_progress::{
            GenesisMetadata,
            GenesisProgressMutate,
        },
        GenesisDatabase,
    },
    service::genesis::{
        progress::ProgressReporter,
        task_manager::CancellationToken,
    },
};

use super::migration_name;

pub struct ImportTask<Handler, Groups, DbDesc>
where
    DbDesc: DatabaseDescription,
{
    handler: Handler,
    skip: usize,
    groups: Groups,
    db: GenesisDatabase<DbDesc>,
    reporter: ProgressReporter,
}

pub trait ImportTable {
    type TableInSnapshot: TableWithBlueprint;
    type TableBeingWritten: TableWithBlueprint;
    type DbDesc: DatabaseDescription;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()>;
}

impl<Logic, GroupGenerator, DbDesc> ImportTask<Logic, GroupGenerator, DbDesc>
where
    DbDesc: DatabaseDescription,
    Logic: ImportTable<DbDesc = DbDesc>,
    GenesisDatabase<DbDesc>: StorageInspect<GenesisMetadata<DbDesc>>,
{
    pub fn new(
        handler: Logic,
        groups: GroupGenerator,
        db: GenesisDatabase<DbDesc>,
        reporter: ProgressReporter,
    ) -> Self {
        let progress_name =
            migration_name::<Logic::TableInSnapshot, Logic::TableBeingWritten>();
        let skip = match db.storage::<GenesisMetadata<DbDesc>>().get(&progress_name) {
            Ok(Some(idx_last_handled)) => {
                usize::saturating_add(idx_last_handled.into_owned(), 1)
            }
            _ => 0,
        };

        Self {
            handler,
            skip,
            groups,
            db,
            reporter,
        }
    }
}

impl<Logic, GroupGenerator, DbDesc> ImportTask<Logic, GroupGenerator, DbDesc>
where
    DbDesc: DatabaseDescription,
    Logic: ImportTable<DbDesc = DbDesc>,
    GroupGenerator:
        IntoIterator<Item = anyhow::Result<Vec<TableEntry<Logic::TableInSnapshot>>>>,
    GenesisMetadata<DbDesc>: TableWithBlueprint<
        Column = DbDesc::Column,
        Key = str,
        Value = usize,
        OwnedValue = usize,
    >,
    GenesisDatabase<DbDesc>:
        StorageInspect<GenesisMetadata<DbDesc>> + WriteTransaction + Modifiable,
    for<'a> StorageTransaction<&'a mut GenesisDatabase<DbDesc>>:
        StorageMutate<GenesisMetadata<DbDesc>, Error = fuel_core_storage::Error>,
{
    pub fn run(mut self, cancel_token: CancellationToken) -> anyhow::Result<()> {
        let mut db = self.db;
        let mut is_cancelled = cancel_token.is_cancelled();
        self.groups
            .into_iter()
            .enumerate()
            .skip(self.skip)
            .take_while(|_| {
                is_cancelled = cancel_token.is_cancelled();
                !is_cancelled
            })
            .try_for_each(|(index, group)| {
                let group = group?;
                let mut tx = db.write_transaction();
                self.handler.process(group, &mut tx)?;

                GenesisProgressMutate::<DbDesc>::update_genesis_progress(
                    &mut tx,
                    &migration_name::<Logic::TableInSnapshot, Logic::TableBeingWritten>(),
                    index,
                )?;
                tx.commit()?;
                self.reporter.set_index(index);
                anyhow::Result::<_>::Ok(())
            })?;

        if is_cancelled {
            bail!("Import cancelled")
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        database::{
            genesis_progress::GenesisProgressInspect,
            GenesisDatabase,
        },
        service::genesis::{
            importer::{
                import_task::ImportTask,
                migration_name,
            },
            progress::ProgressReporter,
            task_manager::CancellationToken,
        },
    };
    use std::sync::{
        Arc,
        Mutex,
    };

    use anyhow::{
        anyhow,
        bail,
    };
    use fuel_core_chain_config::{
        Randomize,
        TableEntry,
    };
    use fuel_core_storage::{
        column::Column,
        iter::{
            BoxedIter,
            IterDirection,
            IterableStore,
        },
        kv_store::{
            KVItem,
            KeyItem,
            KeyValueInspect,
            Value,
        },
        tables::Coins,
        transactional::{
            Changes,
            StorageTransaction,
        },
        Result as StorageResult,
        StorageAsMut,
        StorageAsRef,
        StorageInspect,
    };
    use fuel_core_types::{
        entities::coins::coin::{
            CompressedCoin,
            CompressedCoinV1,
        },
        fuel_tx::UtxoId,
        fuel_types::BlockHeight,
    };
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use crate::{
        database::{
            database_description::on_chain::OnChain,
            genesis_progress::GenesisProgressMutate,
        },
        state::{
            in_memory::memory_store::MemoryStore,
            IterableKeyValueView,
            KeyValueView,
            TransactableStorage,
        },
    };

    use super::ImportTable;

    struct TestHandler<L> {
        logic: L,
    }

    impl<L> TestHandler<L>
    where
        TestHandler<L>: ImportTable,
    {
        pub fn new(logic: L) -> Self {
            Self { logic }
        }
    }

    impl<L> ImportTable for TestHandler<L>
    where
        L: FnMut(
            TableEntry<Coins>,
            &mut StorageTransaction<&mut GenesisDatabase>,
        ) -> anyhow::Result<()>,
    {
        type TableInSnapshot = Coins;
        type TableBeingWritten = Coins;
        type DbDesc = OnChain;
        fn process(
            &mut self,
            group: Vec<TableEntry<Self::TableInSnapshot>>,
            tx: &mut StorageTransaction<&mut GenesisDatabase>,
        ) -> anyhow::Result<()> {
            group
                .into_iter()
                .try_for_each(|item| (self.logic)(item, tx))
        }
    }

    struct TestData {
        batches: Vec<Vec<TableEntry<Coins>>>,
    }

    impl TestData {
        pub fn new(amount: usize) -> Self {
            let mut rng = StdRng::seed_from_u64(0);
            let batches = std::iter::repeat_with(|| TableEntry::randomize(&mut rng))
                .take(amount)
                .map(|el| vec![el])
                .collect();
            Self { batches }
        }

        pub fn as_entries(&self, skip_batches: usize) -> Vec<TableEntry<Coins>> {
            self.batches
                .iter()
                .skip(skip_batches)
                .flat_map(|batch| batch.clone())
                .collect()
        }

        pub fn as_groups(&self) -> Vec<Vec<TableEntry<Coins>>> {
            self.batches.clone()
        }

        pub fn as_ok_groups(&self) -> Vec<anyhow::Result<Vec<TableEntry<Coins>>>> {
            self.as_groups().into_iter().map(Ok).collect()
        }
    }

    #[test]
    fn will_go_through_all_groups() {
        // given
        let data = TestData::new(3);

        let mut called_with = vec![];
        let runner = ImportTask::new(
            TestHandler::new(|group, _| {
                called_with.push(group);
                Ok(())
            }),
            data.as_ok_groups(),
            GenesisDatabase::default(),
            ProgressReporter::default(),
        );

        // when
        runner.run(never_cancel()).unwrap();

        // then
        assert_eq!(called_with, data.as_entries(0));
    }

    #[test]
    fn will_skip_one_group() {
        // given
        let data = TestData::new(2);

        let mut called_with = vec![];
        let mut db = GenesisDatabase::<OnChain>::default();
        GenesisProgressMutate::<OnChain>::update_genesis_progress(
            &mut db,
            &migration_name::<Coins, Coins>(),
            0,
        )
        .unwrap();
        let runner = ImportTask::new(
            TestHandler::new(|element, _| {
                called_with.push(element);
                Ok(())
            }),
            data.as_ok_groups(),
            db,
            ProgressReporter::default(),
        );

        // when
        runner.run(never_cancel()).unwrap();

        // then
        assert_eq!(called_with, data.as_entries(1));
    }

    #[test]
    fn changes_to_db_by_handler_are_behind_a_transaction() {
        // given
        let groups = TestData::new(1);
        let outer_db = GenesisDatabase::default();
        let utxo_id = UtxoId::new(Default::default(), 0);

        let runner = ImportTask::new(
            TestHandler::new(|_, tx| {
                insert_a_coin(tx, &utxo_id);

                assert!(
                    tx.storage::<Coins>().contains_key(&utxo_id).unwrap(),
                    "Coin should be present in the tx db view"
                );

                assert!(
                    !outer_db
                        .storage_as_ref::<Coins>()
                        .contains_key(&utxo_id)
                        .unwrap(),
                    "Coin should not be present in the outer db "
                );

                Ok(())
            }),
            groups.as_ok_groups(),
            outer_db.clone(),
            ProgressReporter::default(),
        );

        // when
        runner.run(never_cancel()).unwrap();

        // then
        assert!(outer_db
            .storage_as_ref::<Coins>()
            .contains_key(&utxo_id)
            .unwrap());
    }

    fn insert_a_coin(
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
        utxo_id: &UtxoId,
    ) {
        let coin: CompressedCoin = CompressedCoinV1::default().into();

        tx.storage_as_mut::<Coins>().insert(utxo_id, &coin).unwrap();
    }

    #[test]
    fn tx_reverted_if_handler_fails() {
        // given
        let groups = TestData::new(1);
        let db = GenesisDatabase::default();
        let utxo_id = UtxoId::new(Default::default(), 0);

        let runner = ImportTask::new(
            TestHandler::new(|_, tx| {
                insert_a_coin(tx, &utxo_id);
                bail!("Some error")
            }),
            groups.as_ok_groups(),
            db.clone(),
            ProgressReporter::default(),
        );

        // when
        let _ = runner.run(never_cancel());

        // then
        assert!(!StorageInspect::<Coins>::contains_key(&db, &utxo_id).unwrap());
    }

    #[test]
    fn handler_failure_is_propagated() {
        // given
        let groups = TestData::new(1);
        let runner = ImportTask::new(
            TestHandler::new(|_, _| bail!("Some error")),
            groups.as_ok_groups(),
            Default::default(),
            ProgressReporter::default(),
        );

        // when
        let result = runner.run(never_cancel());

        // then
        assert!(result.is_err());
    }

    #[test]
    fn seeing_an_invalid_group_propagates_the_error() {
        // given
        let groups = [Err(anyhow!("Some error"))];
        let runner = ImportTask::new(
            TestHandler::new(|_, _| Ok(())),
            groups,
            Default::default(),
            ProgressReporter::default(),
        );

        // when
        let result = runner.run(never_cancel());

        // then
        assert!(result.is_err());
    }

    #[test]
    fn successfully_processed_batch_updates_the_genesis_progress() {
        // given
        let data = TestData::new(2);
        let db = GenesisDatabase::default();
        let runner = ImportTask::new(
            TestHandler::new(|_, _| Ok(())),
            data.as_ok_groups(),
            db.clone(),
            ProgressReporter::default(),
        );

        // when
        runner.run(never_cancel()).unwrap();

        // then
        assert_eq!(
            GenesisProgressInspect::<OnChain>::genesis_progress(
                &db,
                &migration_name::<Coins, Coins>(),
            ),
            Some(1)
        );
    }

    #[tokio::test]
    async fn processing_stops_when_cancelled() {
        // given
        let (tx, rx) = std::sync::mpsc::channel();

        let read_groups = Arc::new(Mutex::new(vec![]));
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let runner = {
            let read_groups = Arc::clone(&read_groups);
            ImportTask::new(
                TestHandler::new(move |el, _| {
                    read_groups.lock().unwrap().push(el);
                    Ok(())
                }),
                rx,
                Default::default(),
                ProgressReporter::default(),
            )
        };

        let token = CancellationToken::new(cancel_token.clone());
        let runner_handle = std::thread::spawn(move || runner.run(token));

        let data = TestData::new(4);
        let take = 3;
        for group in data.as_ok_groups().into_iter().take(take) {
            tx.send(group).unwrap();
        }

        while read_groups.lock().unwrap().len() < take {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        cancel_token.cancel();

        // when
        tx.send(data.as_ok_groups().pop().unwrap()).unwrap();

        // then
        // runner should finish
        drop(tx);
        runner_handle
            .join()
            .unwrap()
            .expect_err("Cancelling is an error");

        // group after signal is not read
        let read_entries = read_groups.lock().unwrap().clone();
        let inserted_groups = data
            .as_entries(0)
            .into_iter()
            .take(take)
            .collect::<Vec<_>>();
        assert_eq!(read_entries, inserted_groups);
    }

    #[derive(Debug)]
    struct BrokenTransactions {
        store: MemoryStore,
    }

    impl BrokenTransactions {
        fn new() -> Self {
            Self {
                store: MemoryStore::default(),
            }
        }
    }

    impl KeyValueInspect for BrokenTransactions {
        type Column = Column;

        fn get(&self, key: &[u8], column: Column) -> StorageResult<Option<Value>> {
            self.store.get(key, column)
        }
    }

    impl IterableStore for BrokenTransactions {
        fn iter_store(
            &self,
            _: Self::Column,
            _: Option<&[u8]>,
            _: Option<&[u8]>,
            _: IterDirection,
        ) -> BoxedIter<KVItem> {
            unimplemented!()
        }

        fn iter_store_keys(
            &self,
            _: Self::Column,
            _: Option<&[u8]>,
            _: Option<&[u8]>,
            _: IterDirection,
        ) -> BoxedIter<KeyItem> {
            unimplemented!()
        }
    }

    impl TransactableStorage<BlockHeight> for BrokenTransactions {
        fn commit_changes(
            &self,
            _: Option<BlockHeight>,
            _: Changes,
        ) -> StorageResult<()> {
            Err(anyhow::anyhow!("I refuse to work!").into())
        }

        fn view_at_height(
            &self,
            _: &BlockHeight,
        ) -> StorageResult<KeyValueView<Self::Column>> {
            Err(anyhow::anyhow!("I refuse to work!").into())
        }

        fn latest_view(&self) -> StorageResult<IterableKeyValueView<Self::Column>> {
            Err(anyhow::anyhow!("I refuse to work!").into())
        }

        fn rollback_block_to(&self, _: &BlockHeight) -> StorageResult<()> {
            Err(anyhow::anyhow!("I refuse to work!").into())
        }
    }

    #[test]
    fn tx_commit_failure_is_propagated() {
        // given
        let groups = TestData::new(1);
        let runner = ImportTask::new(
            TestHandler::new(|_, _| Ok(())),
            groups.as_ok_groups(),
            GenesisDatabase::new(Arc::new(BrokenTransactions::new())),
            ProgressReporter::default(),
        );

        // when
        let result = runner.run(never_cancel());

        // then
        assert!(result.is_err());
    }

    fn never_cancel() -> CancellationToken {
        CancellationToken::new(tokio_util::sync::CancellationToken::new())
    }
}
