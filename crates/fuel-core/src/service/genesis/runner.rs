use fuel_core_chain_config::Group;
use fuel_core_storage::transactional::Transaction;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

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
    resource: GenesisResource,
    handler: Handler,
    tx_opener: TxOpener,
    skip: usize,
    groups: Groups,
    stop_signal: Option<Arc<Notify>>,
    cancel_token: CancellationToken,
}

pub trait ProcessState<T> {
    fn process(&mut self, item: T, tx: &mut Database) -> anyhow::Result<()>;
}

pub trait ProcessStateGroup<T> {
    fn process_group(&mut self, group: Vec<T>, tx: &mut Database) -> anyhow::Result<()>;
}

impl<T, K> ProcessStateGroup<K> for T
where
    T: ProcessState<K>,
{
    fn process_group(&mut self, item: Vec<K>, tx: &mut Database) -> anyhow::Result<()> {
        item.into_iter().try_for_each(|item| self.process(item, tx))
    }
}

pub trait HandlesGenesisResource {
    fn genesis_resource() -> GenesisResource;
}

impl<Logic, GroupGenerator, TxOpener, Item> GenesisRunner<Logic, GroupGenerator, TxOpener>
where
    Logic: ProcessStateGroup<Item>,
    Item: HandlesGenesisResource,
    GroupGenerator: IntoIterator<Item = anyhow::Result<Group<Item>>>,
    TxOpener: TransactionOpener,
{
    pub fn new(
        stop_signal: Option<Arc<Notify>>,
        cancel_token: CancellationToken,
        handler: Logic,
        groups: GroupGenerator,
        tx_opener: TxOpener,
    ) -> Self {
        let resource = Item::genesis_resource();
        let skip = tx_opener
            .view_only()
            .genesis_progress(&resource)
            .map(|idx_last_handled| idx_last_handled.saturating_add(1))
            .unwrap_or_default();
        Self {
            handler,
            skip,
            groups,
            resource,
            tx_opener,
            stop_signal,
            cancel_token,
        }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let notify_stop = || {
            if let Some(stop_signal) = &self.stop_signal {
                stop_signal.notify_one();
            }
        };

        self.groups
            .into_iter()
            .skip(self.skip)
            .take_while(|_| !self.cancel_token.is_cancelled())
            .try_for_each(|group| {
                let mut tx = self.tx_opener.transaction();
                let group = group?;
                let group_num = group.index;
                self.handler.process_group(group.data, tx.as_mut())?;
                tx.update_genesis_progress(self.resource, group_num)?;
                tx.commit()?;
                Ok(())
            })
            .map_err(|e: anyhow::Error| {
                notify_stop();
                e
            })?;

        notify_stop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            Mutex,
        },
        time::Duration,
    };

    use anyhow::{
        anyhow,
        bail,
    };
    use fuel_core_chain_config::Group;
    use fuel_core_database::Error;
    use fuel_core_storage::{
        iter::IntoBoxedIter,
        tables::Coins,
        StorageAsMut,
        StorageInspect,
    };
    use fuel_core_types::{
        entities::coins::coin::CompressedCoin,
        fuel_tx::UtxoId,
    };
    use tokio::sync::Notify;
    use tokio_util::sync::CancellationToken;

    use crate::{
        database::{
            genesis_progress::GenesisResource,
            transaction::DatabaseTransaction,
            Database,
        },
        service::genesis::runner::{
            GenesisRunner,
            HandlesGenesisResource,
            TransactionOpener,
        },
        state::{
            in_memory::memory_store::MemoryStore,
            BatchOperations,
            KeyValueStore,
            TransactableStorage,
        },
    };

    use super::ProcessState;

    impl<T, K> ProcessState<K> for T
    where
        T: FnMut(K, &mut Database) -> anyhow::Result<()>,
    {
        fn process(&mut self, item: K, tx: &mut Database) -> anyhow::Result<()> {
            self(item, tx)
        }
    }

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
    impl HandlesGenesisResource for usize {
        fn genesis_resource() -> GenesisResource {
            GenesisResource::Coins
        }
    }
    impl HandlesGenesisResource for () {
        fn genesis_resource() -> GenesisResource {
            GenesisResource::Coins
        }
    }

    #[test]
    fn will_skip_groups() {
        // given
        let groups: Vec<Result<Group<usize>, anyhow::Error>> = given_ok_groups(2);
        let mut called_with = vec![];
        let mut db = Database::default();
        db.update_genesis_progress(GenesisResource::Coins, 0)
            .unwrap();

        let runner = GenesisRunner::new(
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |element, _: &mut Database| {
                called_with.push(element);
                Ok(())
            },
            groups,
            db,
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(called_with, vec![1]);
    }

    #[test]
    fn will_go_through_all_groups() {
        // given
        let groups = given_ok_groups(3);
        let mut called_with_groups = vec![];
        let runner = GenesisRunner::new(
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |group, _: &mut Database| {
                called_with_groups.push(group);
                Ok(())
            },
            groups,
            Database::default(),
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(called_with_groups, vec![0, 1, 2]);
    }

    #[test]
    fn changes_to_db_by_handler_are_behind_a_transaction() {
        // given
        let groups = given_ok_groups(1);
        let outer_db = Database::default();
        let utxo_id = UtxoId::new(Default::default(), 0);

        let is_coin_present = |db: &Database| {
            StorageInspect::<Coins>::get(&db, &utxo_id)
                .unwrap()
                .is_some()
        };

        let runner = GenesisRunner::new(
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |_, tx: &mut Database| {
                insert_a_coin(tx, &utxo_id);

                assert!(
                    is_coin_present(tx),
                    "Coin should be present in the tx db view"
                );

                assert!(
                    !is_coin_present(&outer_db),
                    "Coin should not be present in the outer db "
                );

                Ok(())
            },
            groups,
            outer_db.clone(),
        );

        // when
        runner.run().unwrap();

        // then
        assert!(is_coin_present(&outer_db));
    }

    fn insert_a_coin(tx: &mut Database, utxo_id: &UtxoId) {
        let coin = CompressedCoin {
            owner: Default::default(),
            amount: Default::default(),
            asset_id: Default::default(),
            maturity: Default::default(),
            tx_pointer: Default::default(),
        };

        tx.storage_as_mut::<Coins>().insert(utxo_id, &coin).unwrap();
    }

    #[test]
    fn tx_reverted_if_handler_fails() {
        // given
        let groups = given_ok_groups(1);
        let db = Database::default();
        let utxo_id = UtxoId::new(Default::default(), 0);

        let is_coin_present = || {
            StorageInspect::<Coins>::get(&db, &utxo_id)
                .unwrap()
                .is_some()
        };
        let runner = GenesisRunner::new(
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |_, tx: &mut Database| {
                insert_a_coin(tx, &utxo_id);
                bail!("Some error")
            },
            groups,
            db.clone(),
        );

        // when
        let _ = runner.run();

        // then
        assert!(!is_coin_present());
    }

    #[test]
    fn handler_failure_is_propagated() {
        // given
        let groups = given_ok_groups(1);
        let runner = GenesisRunner::new(
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |_, _: &mut Database| bail!("Some error"),
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

        fn delete_all(
            &self,
            _column: crate::database::Column,
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
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |_, _: &mut Database| Ok(()),
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
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |_: (), _: &mut Database| Ok(()),
            groups,
            Database::default(),
        );

        // when
        let result = runner.run();

        // then
        assert!(result.is_err());
    }

    #[test]
    fn succesfully_processed_batch_updates_the_genesis_progress() {
        // given
        let groups = given_ok_groups(2);
        let db = Database::default();
        let runner = GenesisRunner::new(
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |_, _: &mut Database| Ok(()),
            groups,
            db.clone(),
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(db.genesis_progress(&GenesisResource::Coins), Some(1));
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
        let utxo_id = UtxoId::new(Default::default(), 0);

        let is_coin_present = || {
            StorageInspect::<Coins>::get(&db, &utxo_id)
                .unwrap()
                .is_some()
        };

        let runner = GenesisRunner::new(
            Some(Arc::new(Notify::new())),
            CancellationToken::new(),
            |_, tx: &mut Database| {
                insert_a_coin(tx, &utxo_id);
                Ok(())
            },
            groups,
            tx_opener,
        );

        // when
        runner.run().unwrap();

        // then
        assert_eq!(db.genesis_progress(&GenesisResource::Coins), Some(0));
        assert!(is_coin_present());
    }

    #[tokio::test]
    async fn processing_stops_when_cancelled() {
        // given
        let stop_signal = Arc::new(Notify::new());
        let cancel_token = CancellationToken::new();

        let (tx, rx) = std::sync::mpsc::channel();

        let read_groups = Arc::new(Mutex::new(vec![]));
        let runner = {
            let read_groups = Arc::clone(&read_groups);
            GenesisRunner::new(
                Some(Arc::clone(&stop_signal)),
                cancel_token.clone(),
                move |el, _: &mut Database| {
                    read_groups.lock().unwrap().push(el);
                    Ok(())
                },
                rx,
                Database::default(),
            )
        };

        let runner_handle = std::thread::spawn(move || runner.run());

        for group_no in 0..3 {
            tx.send(Ok(Group {
                index: group_no,
                data: vec![group_no],
            }))
            .unwrap();
        }
        while read_groups.lock().unwrap().len() < 3 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        cancel_token.cancel();

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
        assert_eq!(read_groups, vec![0, 1, 2]);

        // stop signal is emitted
        tokio::time::timeout(Duration::from_millis(10), stop_signal.notified())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn emits_stop_signal_on_error() {
        // given
        let stop_signal = Arc::new(Notify::new());
        let groups = [Err(anyhow!("Some error"))];
        let runner = GenesisRunner::new(
            Some(Arc::clone(&stop_signal)),
            CancellationToken::new(),
            |_: (), _: &mut Database| Ok(()),
            groups,
            Database::default(),
        );

        // when
        let result = runner.run();

        // then
        assert!(result.is_err());
        tokio::time::timeout(Duration::from_millis(10), stop_signal.notified())
            .await
            .unwrap();
    }
}
