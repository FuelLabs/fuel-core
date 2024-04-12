use fuel_core_chain_config::{Groups, TableEntry};
use fuel_core_storage::{
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    transactional::{StorageTransaction, WriteTransaction},
    Mappable, StorageAsRef, StorageInspect,
};

use crate::{
    database::{
        database_description::{
            off_chain::OffChain, on_chain::OnChain, DatabaseDescription,
        },
        genesis_progress::{GenesisMetadata, GenesisProgressMutate},
        Database,
    },
    service::genesis::task_manager::CancellationToken,
};

use super::progress::ProgressReporter;

pub trait ImportTable<T: Mappable> {
    fn process_on_chain(
        &mut self,
        _group: Vec<TableEntry<T>>,
        _tx: &mut StorageTransaction<&mut Database<OnChain>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn process_off_chain(
        &mut self,
        _group: Vec<TableEntry<T>>,
        _tx: &mut StorageTransaction<&mut Database<OffChain>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn import_entries<T>(
    cancel_token: CancellationToken,
    mut handler: impl ImportTable<T>,
    groups: impl IntoIterator<Item = anyhow::Result<Vec<TableEntry<T>>>>,
    mut on_chain_db: Database<OnChain>,
    mut off_chain_db: Database<OffChain>,
    reporter: ProgressReporter,
) -> anyhow::Result<bool>
where
    T: TableWithBlueprint,
{
    let skip_on_chain = match on_chain_db
        .storage::<GenesisMetadata<OnChain>>()
        .get(T::column().name())
    {
        Ok(Some(idx_last_handled)) => {
            usize::saturating_add(idx_last_handled.into_owned(), 1)
        }
        _ => 0,
    };

    let skip_off_chain = match off_chain_db
        .storage::<GenesisMetadata<OffChain>>()
        .get(T::column().name())
    {
        Ok(Some(idx_last_handled)) => {
            usize::saturating_add(idx_last_handled.into_owned(), 1)
        }
        _ => 0,
    };

    let mut is_cancelled = cancel_token.is_cancelled();
    let result: anyhow::Result<_> = groups
        .into_iter()
        .enumerate()
        .skip(skip_on_chain)
        .take_while(|_| {
            is_cancelled = cancel_token.is_cancelled();
            !is_cancelled
        })
        .try_for_each(|(index, group)| {
            let group = group?;

            let mut on_chain_tx = on_chain_db.write_transaction();

            handler.process_on_chain(group.clone(), &mut on_chain_tx)?;

            GenesisProgressMutate::<OnChain>::update_genesis_progress(
                &mut on_chain_tx,
                T::column().name(),
                index,
            )?;
            on_chain_tx.commit()?;

            let mut off_chain_tx = off_chain_db.write_transaction();

            handler.process_off_chain(group, &mut off_chain_tx)?;

            GenesisProgressMutate::<OffChain>::update_genesis_progress(
                &mut off_chain_tx,
                T::column().name(),
                index,
            )?;

            off_chain_tx.commit()?;

            reporter.set_progress(index as u64);

            Ok(())
        });

    result?;

    Ok(!is_cancelled)
}

#[cfg(test)]
mod tests {
    use crate::{
        database::{
            database_description::off_chain::OffChain,
            genesis_progress::GenesisProgressInspect,
        },
        service::genesis::{
            importer::{import_task::import_entries, progress::ProgressReporter},
            task_manager::CancellationToken,
        },
    };
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use anyhow::{anyhow, bail};
    use fuel_core_chain_config::{Groups, Randomize, TableEntry};
    use fuel_core_storage::{
        column::Column,
        iter::{BoxedIter, IterDirection, IterableStore},
        kv_store::{KVItem, KeyValueInspect, StorageColumn, Value},
        structured_storage::TableWithBlueprint,
        tables::Coins,
        transactional::{Changes, StorageTransaction},
        Result as StorageResult, StorageAsMut, StorageAsRef, StorageInspect,
    };
    use fuel_core_types::{
        entities::coins::coin::{CompressedCoin, CompressedCoinV1},
        fuel_tx::UtxoId,
        fuel_types::BlockHeight,
    };
    use rand::{rngs::StdRng, SeedableRng};

    use crate::{
        combined_database::CombinedDatabase,
        database::{
            database_description::on_chain::OnChain,
            genesis_progress::GenesisProgressMutate, Database,
        },
        state::{in_memory::memory_store::MemoryStore, TransactableStorage},
    };

    use super::ImportTable;

    #[derive(Default, Clone)]
    struct Spy {
        on_chain_called_with: Arc<Mutex<Vec<Vec<TableEntry<Coins>>>>>,
        off_chain_called_with: Arc<Mutex<Vec<Vec<TableEntry<Coins>>>>>,
    }

    impl Spy {
        fn new() -> Self {
            Self {
                on_chain_called_with: Default::default(),
                off_chain_called_with: Default::default(),
            }
        }

        fn default_importer(
            &self,
        ) -> TestTableImporter<
            fn(
                Vec<TableEntry<Coins>>,
                &mut StorageTransaction<&mut Database<OnChain>>,
            ) -> anyhow::Result<()>,
            fn(
                Vec<TableEntry<Coins>>,
                &mut StorageTransaction<&mut Database<OffChain>>,
            ) -> anyhow::Result<()>,
        > {
            TestTableImporter {
                on_chain: |_, _| Ok(()),
                off_chain: |_, _| Ok(()),
                handler: self.clone(),
            }
        }

        fn custom_importer<OnChainCallback, OffChainCallback>(
            &self,
            on_chain: OnChainCallback,
            off_chain: OffChainCallback,
        ) -> TestTableImporter<OnChainCallback, OffChainCallback> {
            TestTableImporter {
                on_chain,
                off_chain,
                handler: self.clone(),
            }
        }
    }

    struct TestTableImporter<OnChainCallback, OffChainCallback> {
        on_chain: OnChainCallback,
        off_chain: OffChainCallback,
        handler: Spy,
    }

    impl<OnChainCallback, OffChainCallback> ImportTable<Coins>
        for TestTableImporter<OnChainCallback, OffChainCallback>
    where
        OnChainCallback: FnMut(
            Vec<TableEntry<Coins>>,
            &mut StorageTransaction<&mut Database<OnChain>>,
        ) -> anyhow::Result<()>,
        OffChainCallback: FnMut(
            Vec<TableEntry<Coins>>,
            &mut StorageTransaction<&mut Database<OffChain>>,
        ) -> anyhow::Result<()>,
    {
        fn process_on_chain(
            &mut self,
            group: Vec<TableEntry<Coins>>,
            tx: &mut StorageTransaction<&mut Database<OnChain>>,
        ) -> anyhow::Result<()> {
            self.handler
                .on_chain_called_with
                .lock()
                .unwrap()
                .push(group);
            (self.on_chain)(group, tx)?;
            Ok(())
        }
        fn process_off_chain(
            &mut self,
            group: Vec<TableEntry<Coins>>,
            tx: &mut StorageTransaction<&mut Database<OffChain>>,
        ) -> anyhow::Result<()> {
            self.handler
                .off_chain_called_with
                .lock()
                .unwrap()
                .push(group);
            (self.off_chain)(group, tx)?;
            Ok(())
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

        pub fn as_unwrapped_groups(
            &self,
            skip_batches: usize,
        ) -> Vec<Vec<TableEntry<Coins>>> {
            self.batches.iter().skip(skip_batches).cloned().collect()
        }

        pub fn as_ok_groups(
            &self,
            skip_batches: usize,
        ) -> Vec<anyhow::Result<Vec<TableEntry<Coins>>>> {
            self.as_unwrapped_groups(skip_batches)
                .into_iter()
                .map(Ok)
                .collect()
        }
    }

    #[test]
    fn will_go_through_all_groups() {
        // given
        let data = TestData::new(3);

        let mut called_with = vec![];

        // when
        import_entries(
            CancellationToken::default(),
            Spy::custom(
                |group, _| {
                    called_with.push(group);
                    Ok(())
                },
                |_, _| Ok(()),
            ),
            data.as_ok_groups(0),
            Database::default(),
            Database::default(),
            ProgressReporter::default(),
        )
        .unwrap();

        // then
        assert_eq!(called_with, data.as_unwrapped_groups(0));
    }

    #[test]
    fn will_skip_one_group() {
        // given
        let data = TestData::new(2);

        let mut called_with = vec![];
        let mut db = CombinedDatabase::default();
        let spy = Spy::new();
        GenesisProgressMutate::<OnChain>::update_genesis_progress(
            db.on_chain_mut(),
            Coins::column().name(),
            0,
        )
        .unwrap();

        // when
        import_entries(
            CancellationToken::default(),
            spy.default_importer(),
            data.as_ok_groups(0),
            db.on_chain().clone(),
            db.off_chain().clone(),
            ProgressReporter::default(),
        )
        .unwrap();

        // then
        assert_eq!(called_with, data.as_unwrapped_groups(1));
    }

    fn insert_a_coin(tx: &mut StorageTransaction<&mut Database>, utxo_id: &UtxoId) {
        let coin: CompressedCoin = CompressedCoinV1::default().into();

        tx.storage_as_mut::<Coins>().insert(utxo_id, &coin).unwrap();
    }

    #[test]
    fn tx_reverted_if_handler_fails() {
        // given
        let groups = TestData::new(1);
        let on_chain_db = Database::default();
        let off_chain_db = Database::default();
        let utxo_id = UtxoId::new(Default::default(), 0);

        // when
        let _ = import_entries(
            CancellationToken::default(),
            Spy::custom(
                |_, tx| {
                    insert_a_coin(tx, &utxo_id);
                    bail!("Some error")
                },
                |_, _| Ok(()),
            ),
            groups.as_ok_groups(0),
            on_chain_db.clone(),
            off_chain_db.clone(),
            ProgressReporter::default(),
        )
        .expect_err("should fail");

        // then
        assert!(!StorageInspect::<Coins>::contains_key(&on_chain_db, &utxo_id).unwrap());
    }

    #[test]
    fn handler_failure_is_propagated() {
        // given
        let groups = TestData::new(1);

        // when
        let result = import_entries(
            CancellationToken::default(),
            Spy::custom(|_, _| bail!("Some error"), |_, _| Ok(())),
            groups.as_ok_groups(0),
            Database::default(),
            Database::default(),
            ProgressReporter::default(),
        );

        // then
        assert!(result.is_err());
    }

    #[test]
    fn seeing_an_invalid_group_propagates_the_error() {
        // given
        let groups = Groups::new(vec![Err(anyhow!("Some error"))]);

        // when
        let result = import_entries(
            CancellationToken::default(),
            Spy::custom(|_, _| Ok(()), |_, _| Ok(())),
            groups,
            Database::default(),
            Database::default(),
            ProgressReporter::default(),
        );

        // then
        assert!(result.is_err());
    }

    #[test]
    fn succesfully_processed_batch_updates_the_genesis_progress() {
        // given
        let data = TestData::new(2);
        let on_chain_db = Database::default();
        let off_chain_db = Database::default();

        // when
        import_entries(
            CancellationToken::default(),
            Spy::custom(|_, _| Ok(()), |_, _| Ok(())),
            data.as_ok_groups(0),
            on_chain_db.clone(),
            off_chain_db.clone(),
            ProgressReporter::default(),
        )
        .unwrap();

        // then
        assert_eq!(
            GenesisProgressInspect::<OnChain>::genesis_progress(
                &on_chain_db,
                Coins::column().name(),
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
        let runner_handle = {
            let read_groups = Arc::clone(&read_groups);
            let cancel_token = cancel_token.clone().into();
            std::thread::spawn(move || {
                import_entries(
                    cancel_token,
                    Spy::custom(
                        move |group, _| {
                            read_groups.lock().unwrap().push(group);
                            Ok(())
                        },
                        |_, _| Ok(()),
                    ),
                    rx,
                    Database::default(),
                    Database::default(),
                    ProgressReporter::default(),
                )
            })
        };

        let data = TestData::new(4);
        let take = 3;
        for group in data.as_ok_groups(0).into_iter().take(take) {
            tx.send(group).unwrap();
        }

        while read_groups.lock().unwrap().len() < take {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        cancel_token.cancel();

        // when
        tx.send(data.as_ok_groups(0).pop().unwrap()).unwrap();

        // then
        // runner should finish
        drop(tx);
        let runner_response = runner_handle.join().unwrap();
        assert!(
            runner_response.is_ok(),
            "Stopping a runner should not be an error"
        );

        // group after signal is not read
        let read_entries = read_groups.lock().unwrap().clone();
        let inserted_groups = &data.as_unwrapped_groups(0)[..take];
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
    }

    impl TransactableStorage<BlockHeight> for BrokenTransactions {
        fn commit_changes(
            &self,
            _: Option<BlockHeight>,
            _: Changes,
        ) -> StorageResult<()> {
            Err(anyhow::anyhow!("I refuse to work!").into())
        }
    }

    #[test]
    fn tx_commit_failure_is_propagated() {
        // given
        let groups = TestData::new(1);

        // TODO: check off chain as well
        // when
        let result = import_entries(
            CancellationToken::default(),
            Spy::custom(|_, _| Ok(()), |_, _| Ok(())),
            groups.as_ok_groups(0),
            Database::new(Arc::new(BrokenTransactions::new())),
            Database::default(),
            ProgressReporter::default(),
        );

        // then
        assert!(result.is_err());
    }
}
