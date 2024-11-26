use fuel_core_storage::{
    Error as StorageError,
    StorageAsMut,
};

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
            owner: coin.owner.clone(),
            asset_id: coin.asset_id.clone(),
            amount: coin.amount,
            utxo_id: coin.utxo_id.clone(),
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
            owner: coin.owner.clone(),
            asset_id: coin.asset_id.clone(),
            amount: coin.amount,
            utxo_id: coin.utxo_id.clone(),
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
            owner: message.recipient().clone(),
            amount: message.amount(),
            nonce: message.nonce().clone(),
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
            owner: message.recipient().clone(),
            amount: message.amount(),
            nonce: message.nonce().clone(),
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
                coins_to_spend::update,
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

    #[test]
    fn coins_to_spend_indexation_enabled_flag_is_respected() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> =
            Database::open_rocksdb(tmp_dir.path(), None, Default::default(), 512)
                .unwrap();
        let mut tx = db.write_transaction();

        const COINS_TO_SPEND_INDEX_IS_DISABLED: bool = false;

        let owner_1 = Address::from([1; 32]);
        let owner_2 = Address::from([2; 32]);

        let asset_id_1 = AssetId::from([11; 32]);
        let asset_id_2 = AssetId::from([12; 32]);

        let coin_1 = make_coin(&owner_1, &asset_id_1, 100);
        let coin_2 = make_coin(&owner_1, &asset_id_2, 200);
        let message_1 = make_retryable_message(&owner_1, 300);
        let message_2 = make_nonretryable_message(&owner_2, 400);

        let base_asset_id = AssetId::from([0; 32]);

        // Initial set of coins
        // TODO[RC]: No .clone() required for coins? Double check the types used, maybe we want `MessageCoin` for messages?
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
}
