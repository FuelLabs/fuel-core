use fuel_core_chain_config::{
    self,
    Group,
};
use fuel_core_storage::transactional::Transaction;
use std::{
    iter::Skip,
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
    },
};

use crate::database::{
    genesis_progress::GenesisResource,
    transaction::DatabaseTransaction,
    Database,
};

pub trait TransactionOpener {
    fn transaction(&mut self) -> DatabaseTransaction;
    fn view_only(&self) -> &Database;
}

impl TransactionOpener for Database {
    fn transaction(&mut self) -> DatabaseTransaction {
        Database::transaction(self)
    }

    fn view_only(&self) -> &Database {
        self
    }
}

pub struct GenesisRunner<Handler, Groups, TxOpener> {
    pub(crate) resource: GenesisResource,
    pub(crate) handler: Handler,
    pub(crate) tx_opener: TxOpener,
    pub(crate) groups: Skip<Groups>,
    pub(crate) stop_signal: Arc<AtomicBool>,
}

impl<Handler, Groups, GroupItem, TxOpener> GenesisRunner<Handler, Groups, TxOpener>
where
    Handler: FnMut(
        fuel_core_chain_config::Group<GroupItem>,
        &mut Database,
    ) -> anyhow::Result<()>,
    Groups: Iterator<Item = anyhow::Result<Group<GroupItem>>>,
    TxOpener: TransactionOpener,
{
    pub(crate) fn new(
        stop_signal: Arc<AtomicBool>,
        resource: GenesisResource,
        handler: Handler,
        groups: impl IntoIterator<IntoIter = Groups>,
        tx_opener: TxOpener,
    ) -> Self {
        let skip = tx_opener.view_only().genesis_progress(&resource);
        let groups = groups.into_iter().skip(skip);
        Self {
            handler,
            groups,
            resource,
            tx_opener,
            stop_signal,
        }
    }

    pub(crate) fn run(mut self) -> anyhow::Result<()> {
        self.groups
            .take_while(|_| !self.stop_signal.load(Ordering::Relaxed))
            .try_for_each(|group| {
                let mut tx = self.tx_opener.transaction();
                (self.handler)(group?, tx.as_mut())?;
                tx.increment_genesis_progress(self.resource)?;
                tx.commit()?;
                Ok(())
            })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::AtomicBool,
        Arc,
        Mutex,
    };

    use anyhow::{
        anyhow,
        bail,
    };
    use fuel_core_chain_config::Group;
    use fuel_core_database::Error;
    use fuel_core_storage::iter::IntoBoxedIter;

    use crate::{
        database::{
            genesis_progress::GenesisResource,
            transaction::DatabaseTransaction,
            Database,
        },
        service::genesis::runner::{
            GenesisRunner,
            TransactionOpener,
        },
        state::{
            in_memory::memory_store::MemoryStore,
            BatchOperations,
            KeyValueStore,
            TransactableStorage,
        },
    };

    fn given_ok_groups(amount: usize) -> Vec<anyhow::Result<Group<usize>>> {
        given_groups(amount).into_iter().map(Ok).collect()
    }

    fn given_groups(amount: usize) -> Vec<Group<usize>> {
        (0..amount)
            .map(|i| Group {
                index: i,
                data: vec![i],
            })
            .collect()
    }

    #[test]
    fn will_skip_groups() {
        // given
        let groups = given_ok_groups(2);
        let mut called_with_groups = vec![];
        let mut db = Database::default();
        let genesis_subtype = GenesisResource::Coins;
        db.increment_genesis_progress(genesis_subtype).unwrap();

        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            genesis_subtype,
            |group, _| {
                called_with_groups.push(group);
                Ok(())
            },
            groups,
            db,
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(
            called_with_groups,
            vec![Group {
                index: 1,
                data: vec![1]
            }]
        );
    }

    #[test]
    fn will_go_through_all_groups() {
        // given
        let groups = given_ok_groups(3);
        let mut called_with_groups = vec![];
        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            GenesisResource::Coins,
            |group, _| {
                called_with_groups.push(group);
                Ok(())
            },
            groups,
            Database::default(),
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(
            called_with_groups,
            vec![
                Group {
                    index: 0,
                    data: vec![0]
                },
                Group {
                    index: 1,
                    data: vec![1]
                },
                Group {
                    index: 2,
                    data: vec![2]
                },
            ]
        );
    }

    #[test]
    fn changes_to_db_by_handler_are_behind_a_transaction() {
        // given
        let groups = given_ok_groups(1);
        let outer_db = Database::default();

        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            GenesisResource::Coins,
            |_, tx| {
                tx.increment_genesis_progress(GenesisResource::Messages)
                    .unwrap();

                assert_eq!(tx.genesis_progress(&GenesisResource::Messages), 1);
                assert_eq!(outer_db.genesis_progress(&GenesisResource::Messages), 0);
                Ok(())
            },
            groups,
            outer_db.clone(),
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(outer_db.genesis_progress(&GenesisResource::Messages), 1);
    }

    #[test]
    fn tx_reverted_if_handler_fails() {
        // given
        let groups = given_ok_groups(1);
        let db = Database::default();
        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            GenesisResource::Coins,
            |_, tx| {
                tx.increment_genesis_progress(GenesisResource::Coins)
                    .unwrap();
                bail!("Some error")
            },
            groups,
            db.clone(),
        );

        // when
        let _ = runner.run();

        // then
        assert_eq!(db.genesis_progress(&GenesisResource::Coins), 0);
    }

    #[test]
    fn handler_failure_is_propagated() {
        // given
        let groups = given_ok_groups(1);
        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            GenesisResource::Coins,
            |_, _| bail!("Some error"),
            groups,
            Database::default(),
        );

        // when
        let result = runner.run();

        // then
        assert!(result.is_err());
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

    impl KeyValueStore for BrokenTransactions {
        fn put(
            &self,
            key: &[u8],
            column: crate::database::Column,
            value: crate::state::Value,
        ) -> crate::database::Result<Option<crate::state::Value>> {
            self.store.put(key, column, value)
        }

        fn write(
            &self,
            key: &[u8],
            column: crate::database::Column,
            buf: &[u8],
        ) -> crate::database::Result<usize> {
            self.store.write(key, column, buf)
        }

        fn replace(
            &self,
            key: &[u8],
            column: crate::database::Column,
            buf: &[u8],
        ) -> crate::database::Result<(usize, Option<crate::state::Value>)> {
            self.store.replace(key, column, buf)
        }

        fn take(
            &self,
            key: &[u8],
            column: crate::database::Column,
        ) -> crate::database::Result<Option<crate::state::Value>> {
            self.store.take(key, column)
        }

        fn delete(
            &self,
            key: &[u8],
            column: crate::database::Column,
        ) -> crate::database::Result<Option<crate::state::Value>> {
            self.store.delete(key, column)
        }

        fn exists(
            &self,
            key: &[u8],
            column: crate::database::Column,
        ) -> crate::database::Result<bool> {
            self.store.exists(key, column)
        }

        fn size_of_value(
            &self,
            key: &[u8],
            column: crate::database::Column,
        ) -> crate::database::Result<Option<usize>> {
            self.store.size_of_value(key, column)
        }

        fn get(
            &self,
            key: &[u8],
            column: crate::database::Column,
        ) -> crate::database::Result<Option<crate::state::Value>> {
            self.store.get(key, column)
        }

        fn read(
            &self,
            key: &[u8],
            column: crate::database::Column,
            buf: &mut [u8],
        ) -> crate::database::Result<Option<usize>> {
            self.store.read(key, column, buf)
        }

        fn read_alloc(
            &self,
            key: &[u8],
            column: crate::database::Column,
        ) -> crate::database::Result<Option<crate::state::Value>> {
            self.store.read_alloc(key, column)
        }

        fn iter_all(
            &self,
            column: crate::database::Column,
            prefix: Option<&[u8]>,
            start: Option<&[u8]>,
            direction: fuel_core_storage::iter::IterDirection,
        ) -> fuel_core_storage::iter::BoxedIter<crate::state::KVItem> {
            self.store
                .iter_all(column, prefix, start, direction)
                .into_boxed()
        }
    }

    impl BatchOperations for BrokenTransactions {
        fn batch_write(
            &self,
            _entries: &mut dyn Iterator<
                Item = (
                    Vec<u8>,
                    crate::database::Column,
                    crate::state::WriteOperation,
                ),
            >,
        ) -> crate::database::Result<()> {
            Err(Error::Other(anyhow!("I refuse to work!")))
        }
    }

    impl TransactableStorage for BrokenTransactions {
        fn flush(&self) -> crate::database::Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn tx_commit_failure_is_propagated() {
        // given
        let groups = given_ok_groups(1);
        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            GenesisResource::Coins,
            |_, _| Ok(()),
            groups,
            Database::new(Arc::new(BrokenTransactions::new())),
        );
        // when
        let result = runner.run();

        // then
        assert!(result.is_err());
    }

    #[test]
    fn seeing_an_invalid_group_propagates_the_error() {
        // given
        let groups = [Err(anyhow!("Some error"))];
        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            GenesisResource::Coins,
            |_: Group<()>, _| Ok(()),
            groups,
            Database::default(),
        );

        // when
        let result = runner.run();

        // then
        assert!(result.is_err());
    }

    #[test]
    fn succesfully_processed_batch_increases_the_genesis_progress() {
        // given
        let groups = given_ok_groups(1);
        let db = Database::default();
        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            GenesisResource::Messages,
            |_, _| Ok(()),
            groups,
            db.clone(),
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(db.genesis_progress(&GenesisResource::Messages), 1);
    }

    #[test]
    fn genesis_progress_is_increased_in_same_transaction_as_batch_work() {
        struct OnlyOneTransactionAllowed {
            db: Database,
            counter: usize,
        }
        impl TransactionOpener for OnlyOneTransactionAllowed {
            fn transaction(&mut self) -> DatabaseTransaction {
                if self.counter == 0 {
                    self.counter += 1;
                    Database::transaction(&self.db)
                } else {
                    panic!("Only one transaction should be opened")
                }
            }

            fn view_only(&self) -> &Database {
                &self.db
            }
        }

        // given
        let groups = given_ok_groups(1);
        let db = Database::default();
        let tx_opener = OnlyOneTransactionAllowed {
            db: db.clone(),
            counter: 0,
        };

        let runner = GenesisRunner::new(
            Arc::new(AtomicBool::new(false)),
            GenesisResource::Coins,
            |_, tx| {
                tx.increment_genesis_progress(GenesisResource::Messages)
                    .unwrap();
                Ok(())
            },
            groups,
            tx_opener,
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(db.genesis_progress(&GenesisResource::Messages), 1);
    }

    #[test]
    fn processing_stops_when_stop_signal_is_given() {
        // given
        let stop_signal = Arc::new(AtomicBool::new(false));

        let (tx, rx) = std::sync::mpsc::channel();

        let read_groups = Arc::new(Mutex::new(vec![]));
        let runner = {
            let read_groups = Arc::clone(&read_groups);
            GenesisRunner::new(
                Arc::clone(&stop_signal),
                GenesisResource::Coins,
                move |el: Group<usize>, _| {
                    read_groups.lock().unwrap().push(el);
                    Ok(())
                },
                rx,
                Database::default(),
            )
        };

        let runner_handle = std::thread::spawn(move || runner.run());

        let groups_that_should_be_read = given_groups(3);
        for group in groups_that_should_be_read.clone() {
            tx.send(Ok(group)).unwrap();
        }
        while read_groups.lock().unwrap().len() < 3 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        stop_signal.store(true, std::sync::atomic::Ordering::Relaxed);

        // when
        tx.send(Ok(Group {
            index: 3,
            data: vec![3],
        }))
        .unwrap();

        // then

        // runner should finish
        drop(tx);
        let runner_response = runner_handle.join().unwrap();
        assert!(
            runner_response.is_ok(),
            "Stopping a runner should not be an error"
        );

        // group after signal is not read
        let read_groups = read_groups.lock().unwrap().clone();
        assert_eq!(read_groups, groups_that_should_be_read);
    }
}
