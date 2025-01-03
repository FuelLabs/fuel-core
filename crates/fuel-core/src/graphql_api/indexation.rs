use fuel_core_storage::{
    Error as StorageError,
    StorageAsMut,
};
use fuel_core_types::{
    entities::{
        coins::coin::Coin,
        Message,
    },
    fuel_tx::{
        Address,
        AssetId,
    },
    services::executor::Event,
};

use super::{
    ports::worker::OffChainDatabaseTransaction,
    storage::balances::{
        CoinBalances,
        CoinBalancesKey,
        MessageBalance,
        MessageBalances,
    },
};

#[derive(derive_more::From, derive_more::Display, Debug)]
pub enum IndexationError {
    #[display(
        fmt = "Coin balance would underflow for owner: {}, asset_id: {}, current_amount: {}, requested_deduction: {}",
        owner,
        asset_id,
        current_amount,
        requested_deduction
    )]
    CoinBalanceWouldUnderflow {
        owner: Address,
        asset_id: AssetId,
        current_amount: u128,
        requested_deduction: u128,
    },
    #[display(
        fmt = "Message balance would underflow for owner: {}, current_amount: {}, requested_deduction: {}, retryable: {}",
        owner,
        current_amount,
        requested_deduction,
        retryable
    )]
    MessageBalanceWouldUnderflow {
        owner: Address,
        current_amount: u128,
        requested_deduction: u128,
        retryable: bool,
    },
    #[from]
    StorageError(StorageError),
}

fn increase_message_balance<T>(
    block_st_transaction: &mut T,
    message: &Message,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = message.recipient();
    let storage = block_st_transaction.storage::<MessageBalances>();
    let current_balance = storage.get(key)?.unwrap_or_default().into_owned();
    let MessageBalance {
        mut retryable,
        mut non_retryable,
    } = current_balance;
    if message.is_retryable_message() {
        retryable = retryable.saturating_add(message.amount() as u128);
    } else {
        non_retryable = non_retryable.saturating_add(message.amount() as u128);
    }
    let new_balance = MessageBalance {
        retryable,
        non_retryable,
    };

    block_st_transaction
        .storage::<MessageBalances>()
        .insert(key, &new_balance)
        .map_err(Into::into)
}

fn decrease_message_balance<T>(
    block_st_transaction: &mut T,
    message: &Message,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = message.recipient();
    let storage = block_st_transaction.storage::<MessageBalances>();
    let MessageBalance {
        retryable,
        non_retryable,
    } = storage.get(key)?.unwrap_or_default().into_owned();
    let current_balance = if message.is_retryable_message() {
        retryable
    } else {
        non_retryable
    };

    let new_amount = current_balance
        .checked_sub(message.amount() as u128)
        .ok_or_else(|| IndexationError::MessageBalanceWouldUnderflow {
            owner: *message.recipient(),
            current_amount: current_balance,
            requested_deduction: message.amount() as u128,
            retryable: message.is_retryable_message(),
        })?;

    let new_balance = if message.is_retryable_message() {
        MessageBalance {
            retryable: new_amount,
            non_retryable,
        }
    } else {
        MessageBalance {
            retryable,
            non_retryable: new_amount,
        }
    };
    block_st_transaction
        .storage::<MessageBalances>()
        .insert(key, &new_balance)
        .map_err(Into::into)
}

fn increase_coin_balance<T>(
    block_st_transaction: &mut T,
    coin: &Coin,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = CoinBalancesKey::new(&coin.owner, &coin.asset_id);
    let storage = block_st_transaction.storage::<CoinBalances>();
    let current_amount = storage.get(&key)?.unwrap_or_default().into_owned();
    let new_amount = current_amount.saturating_add(coin.amount as u128);

    block_st_transaction
        .storage::<CoinBalances>()
        .insert(&key, &new_amount)
        .map_err(Into::into)
}

fn decrease_coin_balance<T>(
    block_st_transaction: &mut T,
    coin: &Coin,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = CoinBalancesKey::new(&coin.owner, &coin.asset_id);
    let storage = block_st_transaction.storage::<CoinBalances>();
    let current_amount = storage.get(&key)?.unwrap_or_default().into_owned();

    let new_amount =
        current_amount
            .checked_sub(coin.amount as u128)
            .ok_or_else(|| IndexationError::CoinBalanceWouldUnderflow {
                owner: coin.owner,
                asset_id: coin.asset_id,
                current_amount,
                requested_deduction: coin.amount as u128,
            })?;

    block_st_transaction
        .storage::<CoinBalances>()
        .insert(&key, &new_amount)
        .map_err(Into::into)
}

pub(crate) fn process_balances_update<T>(
    event: &Event,
    block_st_transaction: &mut T,
    balances_enabled: bool,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    if !balances_enabled {
        return Ok(());
    }

    match event {
        Event::MessageImported(message) => {
            increase_message_balance(block_st_transaction, message)
        }
        Event::MessageConsumed(message) => {
            decrease_message_balance(block_st_transaction, message)
        }
        Event::CoinCreated(coin) => increase_coin_balance(block_st_transaction, coin),
        Event::CoinConsumed(coin) => decrease_coin_balance(block_st_transaction, coin),
        Event::ForcedTransactionFailed { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use fuel_core_storage::{
        transactional::WriteTransaction,
        StorageAsMut,
    };
    use fuel_core_types::{
        entities::{
            coins::coin::Coin,
            relayer::message::MessageV1,
            Message,
        },
        fuel_tx::{
            Address,
            AssetId,
        },
        services::executor::Event,
    };

    use crate::{
        database::{
            database_description::off_chain::OffChain,
            Database,
        },
        graphql_api::{
            indexation::{
                process_balances_update,
                IndexationError,
            },
            ports::worker::OffChainDatabaseTransaction,
            storage::balances::{
                CoinBalances,
                CoinBalancesKey,
                MessageBalance,
                MessageBalances,
            },
        },
        state::rocks_db::DatabaseConfig,
    };

    impl PartialEq for IndexationError {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (
                    Self::CoinBalanceWouldUnderflow {
                        owner: l_owner,
                        asset_id: l_asset_id,
                        current_amount: l_current_amount,
                        requested_deduction: l_requested_deduction,
                    },
                    Self::CoinBalanceWouldUnderflow {
                        owner: r_owner,
                        asset_id: r_asset_id,
                        current_amount: r_current_amount,
                        requested_deduction: r_requested_deduction,
                    },
                ) => {
                    l_owner == r_owner
                        && l_asset_id == r_asset_id
                        && l_current_amount == r_current_amount
                        && l_requested_deduction == r_requested_deduction
                }
                (
                    Self::MessageBalanceWouldUnderflow {
                        owner: l_owner,
                        current_amount: l_current_amount,
                        requested_deduction: l_requested_deduction,
                        retryable: l_retryable,
                    },
                    Self::MessageBalanceWouldUnderflow {
                        owner: r_owner,
                        current_amount: r_current_amount,
                        requested_deduction: r_requested_deduction,
                        retryable: r_retryable,
                    },
                ) => {
                    l_owner == r_owner
                        && l_current_amount == r_current_amount
                        && l_requested_deduction == r_requested_deduction
                        && l_retryable == r_retryable
                }
                (Self::StorageError(l0), Self::StorageError(r0)) => l0 == r0,
                _ => false,
            }
        }
    }

    fn make_coin(owner: &Address, asset_id: &AssetId, amount: u64) -> Coin {
        Coin {
            utxo_id: Default::default(),
            owner: *owner,
            amount,
            asset_id: *asset_id,
            tx_pointer: Default::default(),
        }
    }

    fn make_retryable_message(owner: &Address, amount: u64) -> Message {
        Message::V1(MessageV1 {
            sender: Default::default(),
            recipient: *owner,
            nonce: Default::default(),
            amount,
            data: vec![1],
            da_height: Default::default(),
        })
    }

    fn make_nonretryable_message(owner: &Address, amount: u64) -> Message {
        let mut message = make_retryable_message(owner, amount);
        message.set_data(vec![]);
        message
    }

    fn assert_coin_balance<T>(
        tx: &mut T,
        owner: Address,
        asset_id: AssetId,
        expected_balance: u128,
    ) where
        T: OffChainDatabaseTransaction,
    {
        let key = CoinBalancesKey::new(&owner, &asset_id);
        let balance = tx
            .storage::<CoinBalances>()
            .get(&key)
            .expect("should correctly query db")
            .expect("should have balance");

        assert_eq!(*balance, expected_balance);
    }

    fn assert_message_balance<T>(
        tx: &mut T,
        owner: Address,
        expected_balance: MessageBalance,
    ) where
        T: OffChainDatabaseTransaction,
    {
        let balance = tx
            .storage::<MessageBalances>()
            .get(&owner)
            .expect("should correctly query db")
            .expect("should have balance");

        assert_eq!(*balance, expected_balance);
    }

    #[test]
    fn balances_enabled_flag_is_respected() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let mut tx = db.write_transaction();

        const BALANCES_ARE_DISABLED: bool = false;

        let owner_1 = Address::from([1; 32]);
        let owner_2 = Address::from([2; 32]);

        let asset_id_1 = AssetId::from([11; 32]);
        let asset_id_2 = AssetId::from([12; 32]);

        // Initial set of coins
        let events: Vec<Event> = vec![
            Event::CoinCreated(make_coin(&owner_1, &asset_id_1, 100)),
            Event::CoinConsumed(make_coin(&owner_1, &asset_id_2, 200)),
            Event::MessageImported(make_retryable_message(&owner_1, 300)),
            Event::MessageConsumed(make_nonretryable_message(&owner_2, 400)),
        ];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_DISABLED)
                .expect("should process balance");
        });

        let key = CoinBalancesKey::new(&owner_1, &asset_id_1);
        let balance = tx
            .storage::<CoinBalances>()
            .get(&key)
            .expect("should correctly query db");
        assert!(balance.is_none());

        let key = CoinBalancesKey::new(&owner_1, &asset_id_2);
        let balance = tx
            .storage::<CoinBalances>()
            .get(&key)
            .expect("should correctly query db");
        assert!(balance.is_none());

        let balance = tx
            .storage::<MessageBalances>()
            .get(&owner_1)
            .expect("should correctly query db");
        assert!(balance.is_none());

        let balance = tx
            .storage::<MessageBalances>()
            .get(&owner_2)
            .expect("should correctly query db");
        assert!(balance.is_none());
    }

    #[test]
    fn coins() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let mut tx = db.write_transaction();

        const BALANCES_ARE_ENABLED: bool = true;

        let owner_1 = Address::from([1; 32]);
        let owner_2 = Address::from([2; 32]);

        let asset_id_1 = AssetId::from([11; 32]);
        let asset_id_2 = AssetId::from([12; 32]);

        // Initial set of coins
        let events: Vec<Event> = vec![
            Event::CoinCreated(make_coin(&owner_1, &asset_id_1, 100)),
            Event::CoinCreated(make_coin(&owner_1, &asset_id_2, 200)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_1, 300)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_2, 400)),
        ];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        assert_coin_balance(&mut tx, owner_1, asset_id_1, 100);
        assert_coin_balance(&mut tx, owner_1, asset_id_2, 200);
        assert_coin_balance(&mut tx, owner_2, asset_id_1, 300);
        assert_coin_balance(&mut tx, owner_2, asset_id_2, 400);

        // Add some more coins
        let events: Vec<Event> = vec![
            Event::CoinCreated(make_coin(&owner_1, &asset_id_1, 1)),
            Event::CoinCreated(make_coin(&owner_1, &asset_id_2, 2)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_1, 3)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_2, 4)),
        ];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        assert_coin_balance(&mut tx, owner_1, asset_id_1, 101);
        assert_coin_balance(&mut tx, owner_1, asset_id_2, 202);
        assert_coin_balance(&mut tx, owner_2, asset_id_1, 303);
        assert_coin_balance(&mut tx, owner_2, asset_id_2, 404);

        // Consume some coins
        let events: Vec<Event> = vec![
            Event::CoinConsumed(make_coin(&owner_1, &asset_id_1, 100)),
            Event::CoinConsumed(make_coin(&owner_1, &asset_id_2, 200)),
            Event::CoinConsumed(make_coin(&owner_2, &asset_id_1, 300)),
            Event::CoinConsumed(make_coin(&owner_2, &asset_id_2, 400)),
        ];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        assert_coin_balance(&mut tx, owner_1, asset_id_1, 1);
        assert_coin_balance(&mut tx, owner_1, asset_id_2, 2);
        assert_coin_balance(&mut tx, owner_2, asset_id_1, 3);
        assert_coin_balance(&mut tx, owner_2, asset_id_2, 4);
    }

    #[test]
    fn messages() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let mut tx = db.write_transaction();

        const BALANCES_ARE_ENABLED: bool = true;

        let owner_1 = Address::from([1; 32]);
        let owner_2 = Address::from([2; 32]);

        // Initial set of messages
        let events: Vec<Event> = vec![
            Event::MessageImported(make_retryable_message(&owner_1, 100)),
            Event::MessageImported(make_retryable_message(&owner_2, 200)),
            Event::MessageImported(make_nonretryable_message(&owner_1, 300)),
            Event::MessageImported(make_nonretryable_message(&owner_2, 400)),
        ];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        assert_message_balance(
            &mut tx,
            owner_1,
            MessageBalance {
                retryable: 100,
                non_retryable: 300,
            },
        );

        assert_message_balance(
            &mut tx,
            owner_2,
            MessageBalance {
                retryable: 200,
                non_retryable: 400,
            },
        );

        // Add some messages
        let events: Vec<Event> = vec![
            Event::MessageImported(make_retryable_message(&owner_1, 1)),
            Event::MessageImported(make_retryable_message(&owner_2, 2)),
            Event::MessageImported(make_nonretryable_message(&owner_1, 3)),
            Event::MessageImported(make_nonretryable_message(&owner_2, 4)),
        ];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        assert_message_balance(
            &mut tx,
            owner_1,
            MessageBalance {
                retryable: 101,
                non_retryable: 303,
            },
        );

        assert_message_balance(
            &mut tx,
            owner_2,
            MessageBalance {
                retryable: 202,
                non_retryable: 404,
            },
        );

        // Consume some messages
        let events: Vec<Event> = vec![
            Event::MessageConsumed(make_retryable_message(&owner_1, 100)),
            Event::MessageConsumed(make_retryable_message(&owner_2, 200)),
            Event::MessageConsumed(make_nonretryable_message(&owner_1, 300)),
            Event::MessageConsumed(make_nonretryable_message(&owner_2, 400)),
        ];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        assert_message_balance(
            &mut tx,
            owner_1,
            MessageBalance {
                retryable: 1,
                non_retryable: 3,
            },
        );

        assert_message_balance(
            &mut tx,
            owner_2,
            MessageBalance {
                retryable: 2,
                non_retryable: 4,
            },
        );
    }

    #[test]
    fn coin_balance_overflow_does_not_error() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let mut tx = db.write_transaction();

        const BALANCES_ARE_ENABLED: bool = true;

        let owner = Address::from([1; 32]);
        let asset_id = AssetId::from([11; 32]);

        // Make the initial balance huge
        let key = CoinBalancesKey::new(&owner, &asset_id);
        tx.storage::<CoinBalances>()
            .insert(&key, &u128::MAX)
            .expect("should correctly query db");

        assert_coin_balance(&mut tx, owner, asset_id, u128::MAX);

        // Try to add more coins
        let events: Vec<Event> =
            vec![Event::CoinCreated(make_coin(&owner, &asset_id, 1))];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        assert_coin_balance(&mut tx, owner, asset_id, u128::MAX);
    }

    #[test]
    fn message_balance_overflow_does_not_error() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let mut tx = db.write_transaction();

        const BALANCES_ARE_ENABLED: bool = true;
        const MAX_BALANCES: MessageBalance = MessageBalance {
            retryable: u128::MAX,
            non_retryable: u128::MAX,
        };

        let owner = Address::from([1; 32]);

        // Make the initial balance huge
        tx.storage::<MessageBalances>()
            .insert(&owner, &MAX_BALANCES)
            .expect("should correctly query db");

        assert_message_balance(&mut tx, owner, MAX_BALANCES);

        // Try to add more coins
        let events: Vec<Event> = vec![
            Event::MessageImported(make_retryable_message(&owner, 1)),
            Event::MessageImported(make_nonretryable_message(&owner, 1)),
        ];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        assert_message_balance(&mut tx, owner, MAX_BALANCES);
    }

    #[test]
    fn coin_balance_underflow_causes_error() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let mut tx = db.write_transaction();

        const BALANCES_ARE_ENABLED: bool = true;

        let owner = Address::from([1; 32]);
        let asset_id_1 = AssetId::from([11; 32]);
        let asset_id_2 = AssetId::from([12; 32]);

        // Initial set of coins
        let events: Vec<Event> =
            vec![Event::CoinCreated(make_coin(&owner, &asset_id_1, 100))];

        events.iter().for_each(|event| {
            process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED)
                .expect("should process balance");
        });

        // Consume more coins than available
        let events: Vec<Event> = vec![
            Event::CoinConsumed(make_coin(&owner, &asset_id_1, 10000)),
            Event::CoinConsumed(make_coin(&owner, &asset_id_2, 20000)),
        ];

        let expected_errors = vec![
            IndexationError::CoinBalanceWouldUnderflow {
                owner,
                asset_id: asset_id_1,
                current_amount: 100,
                requested_deduction: 10000,
            },
            IndexationError::CoinBalanceWouldUnderflow {
                owner,
                asset_id: asset_id_2,
                current_amount: 0,
                requested_deduction: 20000,
            },
        ];

        let actual_errors: Vec<_> = events
            .iter()
            .map(|event| {
                process_balances_update(event, &mut tx, BALANCES_ARE_ENABLED).unwrap_err()
            })
            .collect();

        assert_eq!(expected_errors, actual_errors);
    }
}
