use fuel_core_storage::StorageAsMut;

use fuel_core_types::{
    entities::{
        coins::coin::Coin,
        Message,
    },
    fuel_tx::AssetId,
    services::executor::Event,
};

use crate::graphql_api::{
    ports::worker::OffChainDatabaseTransaction,
    storage::coins::{
        CoinsToSpendIndex,
        CoinsToSpendIndexKey,
    },
};

use super::error::IndexationError;

// Indicates that a message is retryable.
pub(crate) const RETRYABLE_BYTE: [u8; 1] = [0x00];

// Indicates that a message is non-retryable (also, all coins use this byte).
pub(crate) const NON_RETRYABLE_BYTE: [u8; 1] = [0x01];

// For key disambiguation purposes, the coins use UtxoId as a key suffix (34 bytes).
// Messages do not have UtxoId, hence we use Nonce for differentiation.
// Nonce is 32 bytes, so we need to pad it with 2 bytes to make it 34 bytes.
// We need equal length keys to maintain the correct, lexicographical order of the keys.
pub(crate) const MESSAGE_PADDING_BYTES: [u8; 2] = [0xFF, 0xFF];

#[repr(u8)]
#[derive(Clone)]
pub(crate) enum IndexedCoinType {
    Coin,
    Message,
}

fn add_coin<T>(block_st_transaction: &mut T, coin: &Coin) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = CoinsToSpendIndexKey::from_coin(coin);
    let storage = block_st_transaction.storage::<CoinsToSpendIndex>();
    let maybe_old_value = storage.replace(&key, &(IndexedCoinType::Coin as u8))?;
    if maybe_old_value.is_some() {
        return Err(IndexationError::CoinToSpendAlreadyIndexed {
            owner: coin.owner,
            asset_id: coin.asset_id,
            amount: coin.amount,
            utxo_id: coin.utxo_id,
        });
    }
    Ok(())
}

fn remove_coin<T>(
    block_st_transaction: &mut T,
    coin: &Coin,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = CoinsToSpendIndexKey::from_coin(coin);
    let storage = block_st_transaction.storage::<CoinsToSpendIndex>();
    let maybe_old_value = storage.take(&key)?;
    if maybe_old_value.is_none() {
        return Err(IndexationError::CoinToSpendNotFound {
            owner: coin.owner,
            asset_id: coin.asset_id,
            amount: coin.amount,
            utxo_id: coin.utxo_id,
        });
    }
    Ok(())
}

fn add_message<T>(
    block_st_transaction: &mut T,
    message: &Message,
    base_asset_id: &AssetId,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = CoinsToSpendIndexKey::from_message(message, base_asset_id);
    let storage = block_st_transaction.storage::<CoinsToSpendIndex>();
    let maybe_old_value = storage.replace(&key, &(IndexedCoinType::Message as u8))?;
    if maybe_old_value.is_some() {
        return Err(IndexationError::MessageToSpendAlreadyIndexed {
            owner: *message.recipient(),
            amount: message.amount(),
            nonce: *message.nonce(),
        });
    }
    Ok(())
}

fn remove_message<T>(
    block_st_transaction: &mut T,
    message: &Message,
    base_asset_id: &AssetId,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = CoinsToSpendIndexKey::from_message(message, base_asset_id);
    let storage = block_st_transaction.storage::<CoinsToSpendIndex>();
    let maybe_old_value = storage.take(&key)?;
    if maybe_old_value.is_none() {
        return Err(IndexationError::MessageToSpendNotFound {
            owner: *message.recipient(),
            amount: message.amount(),
            nonce: *message.nonce(),
        });
    }
    Ok(())
}

pub(crate) fn update<T>(
    event: &Event,
    block_st_transaction: &mut T,
    enabled: bool,
    base_asset_id: &AssetId,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    if !enabled {
        return Ok(());
    }

    match event {
        Event::MessageImported(message) => {
            add_message(block_st_transaction, message, base_asset_id)
        }
        Event::MessageConsumed(message) => {
            remove_message(block_st_transaction, message, base_asset_id)
        }
        Event::CoinCreated(coin) => add_coin(block_st_transaction, coin),
        Event::CoinConsumed(coin) => remove_coin(block_st_transaction, coin),
        Event::ForcedTransactionFailed { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use fuel_core_storage::{
        iter::IterDirection,
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
    use rand::seq::SliceRandom;

    use itertools::Itertools;
    use proptest::{
        collection::vec,
        prelude::*,
    };

    use crate::{
        database::{
            database_description::off_chain::OffChain,
            Database,
        },
        graphql_api::{
            indexation::{
                coins_to_spend::{
                    update,
                    RETRYABLE_BYTE,
                },
                error::IndexationError,
                test_utils::{
                    make_coin,
                    make_nonretryable_message,
                    make_retryable_message,
                },
            },
            storage::coins::{
                CoinsToSpendIndex,
                CoinsToSpendIndexKey,
            },
        },
    };

    use super::NON_RETRYABLE_BYTE;

    fn assert_index_entries(
        db: &Database<OffChain>,
        expected_entries: &[(Address, AssetId, [u8; 1], u64)],
    ) {
        let actual_entries: Vec<_> = db
            .entries::<CoinsToSpendIndex>(None, IterDirection::Forward)
            .map(|entry| entry.expect("should read entries"))
            .map(|entry| {
                (
                    entry.key.owner(),
                    entry.key.asset_id(),
                    [entry.key.retryable_flag()],
                    entry.key.amount(),
                )
            })
            .collect();

        assert_eq!(expected_entries, actual_entries.as_slice());
    }

    #[test]
    fn coins_to_spend_indexation_enabled_flag_is_respected() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
                .unwrap();
        let mut tx = db.write_transaction();

        const COINS_TO_SPEND_INDEX_IS_DISABLED: bool = false;
        let base_asset_id = AssetId::from([0; 32]);

        let owner_1 = Address::from([1; 32]);
        let owner_2 = Address::from([2; 32]);

        let asset_id_1 = AssetId::from([11; 32]);
        let asset_id_2 = AssetId::from([12; 32]);

        let coin_1 = make_coin(&owner_1, &asset_id_1, 100);
        let coin_2 = make_coin(&owner_1, &asset_id_2, 200);
        let message_1 = make_retryable_message(&owner_1, 300);
        let message_2 = make_nonretryable_message(&owner_2, 400);

        // TODO[RC]: No clone() required for coins? Double check the types used,
        // maybe we want `MessageCoin` (which is Copy) for messages?
        // 1) Currently we use the same types as embedded in the executor `Event`.
        // 2) `MessageCoin` will refuse to construct itself from a `Message` if the data is empty
        //      impl TryFrom<Message> for MessageCoin { ... if !data.is_empty() ... }
        // Actually it shouldn't matter from the indexation perspective, as we just need
        // to read data from the type and don't care about which data type we took it from.

        // Initial set of coins
        let events: Vec<Event> = vec![
            Event::CoinCreated(coin_1),
            Event::CoinConsumed(coin_2),
            Event::MessageImported(message_1.clone()),
            Event::MessageConsumed(message_2.clone()),
        ];

        events.iter().for_each(|event| {
            update(
                event,
                &mut tx,
                COINS_TO_SPEND_INDEX_IS_DISABLED,
                &base_asset_id,
            )
            .expect("should process balance");
        });

        let key = CoinsToSpendIndexKey::from_coin(&coin_1);
        let coin = tx
            .storage::<CoinsToSpendIndex>()
            .get(&key)
            .expect("should correctly query db");
        assert!(coin.is_none());

        let key = CoinsToSpendIndexKey::from_coin(&coin_2);
        let coin = tx
            .storage::<CoinsToSpendIndex>()
            .get(&key)
            .expect("should correctly query db");
        assert!(coin.is_none());

        let key = CoinsToSpendIndexKey::from_message(&message_1, &base_asset_id);
        let message = tx
            .storage::<CoinsToSpendIndex>()
            .get(&key)
            .expect("should correctly query db");
        assert!(message.is_none());

        let key = CoinsToSpendIndexKey::from_message(&message_2, &base_asset_id);
        let message = tx
            .storage::<CoinsToSpendIndex>()
            .get(&key)
            .expect("should correctly query db");
        assert!(message.is_none());
    }

    #[test]
    fn coin_owner_and_asset_id_is_respected() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
                .unwrap();
        let mut tx = db.write_transaction();

        const COINS_TO_SPEND_INDEX_IS_ENABLED: bool = true;
        let base_asset_id = AssetId::from([0; 32]);

        let owner_1 = Address::from([1; 32]);
        let owner_2 = Address::from([2; 32]);

        let asset_id_1 = AssetId::from([11; 32]);
        let asset_id_2 = AssetId::from([12; 32]);

        // Initial set of coins of the same asset id - mind the random order of amounts
        let mut events: Vec<Event> = vec![
            Event::CoinCreated(make_coin(&owner_1, &asset_id_1, 100)),
            Event::CoinCreated(make_coin(&owner_1, &asset_id_1, 300)),
            Event::CoinCreated(make_coin(&owner_1, &asset_id_1, 200)),
        ];

        // Add more coins, some of them for the new asset id - mind the random order of amounts
        events.extend([
            Event::CoinCreated(make_coin(&owner_1, &asset_id_2, 10)),
            Event::CoinCreated(make_coin(&owner_1, &asset_id_2, 12)),
            Event::CoinCreated(make_coin(&owner_1, &asset_id_2, 11)),
            Event::CoinCreated(make_coin(&owner_1, &asset_id_1, 150)),
        ]);

        // Add another owner into the mix
        events.extend([
            Event::CoinCreated(make_coin(&owner_2, &asset_id_1, 1000)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_1, 2000)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_1, 200000)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_1, 1500)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_2, 900)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_2, 800)),
            Event::CoinCreated(make_coin(&owner_2, &asset_id_2, 700)),
        ]);

        // Consume some coins
        events.extend([
            Event::CoinConsumed(make_coin(&owner_1, &asset_id_1, 300)),
            Event::CoinConsumed(make_coin(&owner_2, &asset_id_1, 200000)),
        ]);

        // Process all events
        events.iter().for_each(|event| {
            update(
                event,
                &mut tx,
                COINS_TO_SPEND_INDEX_IS_ENABLED,
                &base_asset_id,
            )
            .expect("should process coins to spend");
        });
        tx.commit().expect("should commit transaction");

        // Mind the sorted amounts
        let expected_index_entries = &[
            (owner_1, asset_id_1, NON_RETRYABLE_BYTE, 100),
            (owner_1, asset_id_1, NON_RETRYABLE_BYTE, 150),
            (owner_1, asset_id_1, NON_RETRYABLE_BYTE, 200),
            (owner_1, asset_id_2, NON_RETRYABLE_BYTE, 10),
            (owner_1, asset_id_2, NON_RETRYABLE_BYTE, 11),
            (owner_1, asset_id_2, NON_RETRYABLE_BYTE, 12),
            (owner_2, asset_id_1, NON_RETRYABLE_BYTE, 1000),
            (owner_2, asset_id_1, NON_RETRYABLE_BYTE, 1500),
            (owner_2, asset_id_1, NON_RETRYABLE_BYTE, 2000),
            (owner_2, asset_id_2, NON_RETRYABLE_BYTE, 700),
            (owner_2, asset_id_2, NON_RETRYABLE_BYTE, 800),
            (owner_2, asset_id_2, NON_RETRYABLE_BYTE, 900),
        ];

        assert_index_entries(&db, expected_index_entries);
    }

    #[test]
    fn message_owner_is_respected() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
                .unwrap();
        let mut tx = db.write_transaction();

        const COINS_TO_SPEND_INDEX_IS_ENABLED: bool = true;
        let base_asset_id = AssetId::from([0; 32]);

        let owner_1 = Address::from([1; 32]);
        let owner_2 = Address::from([2; 32]);

        // Initial set of coins of the same asset id - mind the random order of amounts
        let mut events: Vec<Event> = vec![
            Event::MessageImported(make_nonretryable_message(&owner_1, 100)),
            Event::MessageImported(make_nonretryable_message(&owner_1, 300)),
            Event::MessageImported(make_nonretryable_message(&owner_1, 200)),
        ];

        // Add another owner into the mix
        events.extend([
            Event::MessageImported(make_nonretryable_message(&owner_2, 1000)),
            Event::MessageImported(make_nonretryable_message(&owner_2, 2000)),
            Event::MessageImported(make_nonretryable_message(&owner_2, 200000)),
            Event::MessageImported(make_nonretryable_message(&owner_2, 800)),
            Event::MessageImported(make_nonretryable_message(&owner_2, 700)),
        ]);

        // Consume some coins
        events.extend([
            Event::MessageConsumed(make_nonretryable_message(&owner_1, 300)),
            Event::MessageConsumed(make_nonretryable_message(&owner_2, 200000)),
        ]);

        // Process all events
        events.iter().for_each(|event| {
            update(
                event,
                &mut tx,
                COINS_TO_SPEND_INDEX_IS_ENABLED,
                &base_asset_id,
            )
            .expect("should process coins to spend");
        });
        tx.commit().expect("should commit transaction");

        // Mind the sorted amounts
        let expected_index_entries = &[
            (owner_1, base_asset_id, NON_RETRYABLE_BYTE, 100),
            (owner_1, base_asset_id, NON_RETRYABLE_BYTE, 200),
            (owner_2, base_asset_id, NON_RETRYABLE_BYTE, 700),
            (owner_2, base_asset_id, NON_RETRYABLE_BYTE, 800),
            (owner_2, base_asset_id, NON_RETRYABLE_BYTE, 1000),
            (owner_2, base_asset_id, NON_RETRYABLE_BYTE, 2000),
        ];

        assert_index_entries(&db, expected_index_entries);
    }

    #[test]
    fn coins_with_retryable_and_non_retryable_messages_are_not_mixed() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
                .unwrap();
        let mut tx = db.write_transaction();

        const COINS_TO_SPEND_INDEX_IS_ENABLED: bool = true;
        let base_asset_id = AssetId::from([0; 32]);
        let owner = Address::from([1; 32]);
        let asset_id = AssetId::from([11; 32]);

        let mut events = vec![
            Event::CoinCreated(make_coin(&owner, &asset_id, 101)),
            Event::CoinCreated(make_coin(&owner, &asset_id, 100)),
            Event::CoinCreated(make_coin(&owner, &base_asset_id, 200000)),
            Event::CoinCreated(make_coin(&owner, &base_asset_id, 201)),
            Event::CoinCreated(make_coin(&owner, &base_asset_id, 200)),
            Event::MessageImported(make_retryable_message(&owner, 301)),
            Event::MessageImported(make_retryable_message(&owner, 200000)),
            Event::MessageImported(make_retryable_message(&owner, 300)),
            Event::MessageImported(make_nonretryable_message(&owner, 401)),
            Event::MessageImported(make_nonretryable_message(&owner, 200000)),
            Event::MessageImported(make_nonretryable_message(&owner, 400)),
        ];
        events.shuffle(&mut rand::thread_rng());

        // Delete the "big" coins
        events.extend([
            Event::CoinConsumed(make_coin(&owner, &base_asset_id, 200000)),
            Event::MessageConsumed(make_retryable_message(&owner, 200000)),
            Event::MessageConsumed(make_nonretryable_message(&owner, 200000)),
        ]);

        // Process all events
        events.iter().for_each(|event| {
            update(
                event,
                &mut tx,
                COINS_TO_SPEND_INDEX_IS_ENABLED,
                &base_asset_id,
            )
            .expect("should process coins to spend");
        });
        tx.commit().expect("should commit transaction");

        // Mind the amounts are always correctly sorted
        let expected_index_entries = &[
            (owner, base_asset_id, RETRYABLE_BYTE, 300),
            (owner, base_asset_id, RETRYABLE_BYTE, 301),
            (owner, base_asset_id, NON_RETRYABLE_BYTE, 200),
            (owner, base_asset_id, NON_RETRYABLE_BYTE, 201),
            (owner, base_asset_id, NON_RETRYABLE_BYTE, 400),
            (owner, base_asset_id, NON_RETRYABLE_BYTE, 401),
            (owner, asset_id, NON_RETRYABLE_BYTE, 100),
            (owner, asset_id, NON_RETRYABLE_BYTE, 101),
        ];

        assert_index_entries(&db, expected_index_entries);
    }

    #[test]
    fn double_insertion_causes_error() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
                .unwrap();
        let mut tx = db.write_transaction();

        const COINS_TO_SPEND_INDEX_IS_ENABLED: bool = true;
        let base_asset_id = AssetId::from([0; 32]);
        let owner = Address::from([1; 32]);
        let asset_id = AssetId::from([11; 32]);

        let coin = make_coin(&owner, &asset_id, 100);
        let coin_event = Event::CoinCreated(coin);

        assert!(update(
            &coin_event,
            &mut tx,
            COINS_TO_SPEND_INDEX_IS_ENABLED,
            &base_asset_id,
        )
        .is_ok());
        assert_eq!(
            update(
                &coin_event,
                &mut tx,
                COINS_TO_SPEND_INDEX_IS_ENABLED,
                &base_asset_id,
            )
            .unwrap_err(),
            IndexationError::CoinToSpendAlreadyIndexed {
                owner,
                asset_id,
                amount: 100,
                utxo_id: coin.utxo_id,
            }
        );

        let message = make_nonretryable_message(&owner, 400);
        let message_event = Event::MessageImported(message.clone());
        assert!(update(
            &message_event,
            &mut tx,
            COINS_TO_SPEND_INDEX_IS_ENABLED,
            &base_asset_id,
        )
        .is_ok());
        assert_eq!(
            update(
                &message_event,
                &mut tx,
                COINS_TO_SPEND_INDEX_IS_ENABLED,
                &base_asset_id,
            )
            .unwrap_err(),
            IndexationError::MessageToSpendAlreadyIndexed {
                owner,
                amount: 400,
                nonce: *message.nonce(),
            }
        );
    }

    #[test]
    fn removal_of_missing_index_entry_causes_error() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
                .unwrap();
        let mut tx = db.write_transaction();

        const COINS_TO_SPEND_INDEX_IS_ENABLED: bool = true;
        let base_asset_id = AssetId::from([0; 32]);
        let owner = Address::from([1; 32]);
        let asset_id = AssetId::from([11; 32]);

        let coin = make_coin(&owner, &asset_id, 100);
        let coin_event = Event::CoinConsumed(coin);
        assert_eq!(
            update(
                &coin_event,
                &mut tx,
                COINS_TO_SPEND_INDEX_IS_ENABLED,
                &base_asset_id,
            )
            .unwrap_err(),
            IndexationError::CoinToSpendNotFound {
                owner,
                asset_id,
                amount: 100,
                utxo_id: coin.utxo_id,
            }
        );

        let message = make_nonretryable_message(&owner, 400);
        let message_event = Event::MessageConsumed(message.clone());
        assert_eq!(
            update(
                &message_event,
                &mut tx,
                COINS_TO_SPEND_INDEX_IS_ENABLED,
                &base_asset_id,
            )
            .unwrap_err(),
            IndexationError::MessageToSpendNotFound {
                owner,
                amount: 400,
                nonce: *message.nonce(),
            }
        );
    }

    #[test]
    fn can_differentiate_between_coin_with_base_asset_id_and_message() {
        let base_asset_id = AssetId::from([0; 32]);
        let owner = Address::from([1; 32]);

        let coin = make_coin(&owner, &base_asset_id, 100);
        let message = make_nonretryable_message(&owner, 100);

        let coin_key = CoinsToSpendIndexKey::from_coin(&coin);
        let message_key = CoinsToSpendIndexKey::from_message(&message, &base_asset_id);

        assert_ne!(coin_key, message_key);
    }

    proptest! {
        #[test]
        fn test_coin_index_is_sorted(
            amounts in vec(any::<u64>(), 1..100),
        ) {
            use tempfile::TempDir;
            let tmp_dir = TempDir::new().unwrap();
            let mut db: Database<OffChain> =
                Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
                    .unwrap();
            let mut tx = db.write_transaction();
            let base_asset_id = AssetId::from([0; 32]);
            const COINS_TO_SPEND_INDEX_IS_ENABLED: bool = true;

            let events: Vec<_> = amounts.iter()
                .map(|&amount| Event::CoinCreated(make_coin(&Address::from([1; 32]), &AssetId::from([11; 32]), amount)))
                .collect();

                events.iter().for_each(|event| {
                    update(
                        event,
                        &mut tx,
                        COINS_TO_SPEND_INDEX_IS_ENABLED,
                        &base_asset_id,
                    )
                    .expect("should process coins to spend");
                });
                tx.commit().expect("should commit transaction");

                let actual_amounts: Vec<_> = db
                    .entries::<CoinsToSpendIndex>(None, IterDirection::Forward)
                    .map(|entry| entry.expect("should read entries"))
                    .map(|entry|
                            entry.key.amount(),
                    )
                    .collect();

                let sorted_amounts = amounts.iter().copied().sorted().collect::<Vec<_>>();

                prop_assert_eq!(sorted_amounts, actual_amounts);
        }
    }
}
