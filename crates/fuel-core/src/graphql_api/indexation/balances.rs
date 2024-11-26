use fuel_core_storage::StorageAsMut;
use fuel_core_types::{
    entities::{
        coins::coin::Coin,
        Message,
    },
    services::executor::Event,
};

use crate::graphql_api::{
    ports::worker::OffChainDatabaseTransaction,
    storage::balances::{
        CoinBalances,
        CoinBalancesKey,
        MessageBalance,
        MessageBalances,
    },
};

use super::error::IndexationError;

fn increase_message_balance<T>(
    block_st_transaction: &mut T,
    message: &Message,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = message.recipient();
    let storage = block_st_transaction.storage::<MessageBalances>();
    let current_balance = storage.get(key)?.unwrap_or_default();
    let MessageBalance {
        mut retryable,
        mut non_retryable,
    } = *current_balance;
    if message.has_retryable_amount() {
        retryable = retryable.saturating_add(message.amount() as u128);
    } else {
        non_retryable = non_retryable.saturating_add(message.amount() as u128);
    }
    let new_balance = MessageBalance {
        retryable,
        non_retryable,
    };

    let storage = block_st_transaction.storage::<MessageBalances>();
    Ok(storage.insert(key, &new_balance)?)
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
    } = *storage.get(key)?.unwrap_or_default();
    let current_balance = if message.has_retryable_amount() {
        retryable
    } else {
        non_retryable
    };

    current_balance
        .checked_sub(message.amount() as u128)
        .ok_or_else(|| IndexationError::MessageBalanceWouldUnderflow {
            owner: *message.recipient(),
            current_amount: current_balance,
            requested_deduction: message.amount() as u128,
            retryable: message.has_retryable_amount(),
        })
        .and_then(|new_amount| {
            let storage = block_st_transaction.storage::<MessageBalances>();
            let new_balance = if message.has_retryable_amount() {
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
            storage.insert(key, &new_balance).map_err(Into::into)
        })
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
    let current_amount = *storage.get(&key)?.unwrap_or_default();
    let new_amount = current_amount.saturating_add(coin.amount as u128);

    let storage = block_st_transaction.storage::<CoinBalances>();
    Ok(storage.insert(&key, &new_amount)?)
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
    let current_amount = *storage.get(&key)?.unwrap_or_default();

    current_amount
        .checked_sub(coin.amount as u128)
        .ok_or_else(|| IndexationError::CoinBalanceWouldUnderflow {
            owner: coin.owner,
            asset_id: coin.asset_id,
            current_amount,
            requested_deduction: coin.amount as u128,
        })
        .and_then(|new_amount| {
            block_st_transaction
                .storage::<CoinBalances>()
                .insert(&key, &new_amount)
                .map_err(Into::into)
        })
}

pub(crate) fn update<T>(
    event: &Event,
    block_st_transaction: &mut T,
    enabled: bool,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    if !enabled {
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
                balances::update,
                error::IndexationError,
                test_utils::{
                    make_coin,
                    make_nonretryable_message,
                    make_retryable_message,
                },
            },
            ports::worker::OffChainDatabaseTransaction,
            storage::balances::{
                CoinBalances,
                CoinBalancesKey,
                MessageBalance,
                MessageBalances,
            },
        },
    };

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
    fn balances_indexation_enabled_flag_is_respected() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
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
            update(event, &mut tx, BALANCES_ARE_DISABLED)
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
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
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
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
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
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
        });

        assert_coin_balance(&mut tx, owner, asset_id, u128::MAX);
    }

    #[test]
    fn message_balance_overflow_does_not_error() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
        });

        assert_message_balance(&mut tx, owner, MAX_BALANCES);
    }

    #[test]
    fn coin_balance_underflow_causes_error() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
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
            update(event, &mut tx, BALANCES_ARE_ENABLED).expect("should process balance");
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
            .map(|event| update(event, &mut tx, BALANCES_ARE_ENABLED).unwrap_err())
            .collect();

        assert_eq!(expected_errors, actual_errors);
    }
}
