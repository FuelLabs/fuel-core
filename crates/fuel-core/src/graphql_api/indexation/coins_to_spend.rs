use fuel_core_storage::{
    Error as StorageError,
    StorageAsMut,
};

use fuel_core_types::{
    entities::coins::coin::Coin,
    services::executor::Event,
};

use crate::graphql_api::{
    ports::worker::OffChainDatabaseTransaction,
    storage::coins::{
        CoinsToSpendIndex,
        CoinsToSpendIndexKey,
    },
};

use super::IndexationError;

// For key disambiguation purposes, the coins use UtxoId as a key suffix (34 bytes).
// Messages do not have UtxoId, hence we use Nonce for differentiation.
// Nonce is 32 bytes, so we need to pad it with 2 bytes to make it 34 bytes.
// We need equal length keys to maintain the correct, lexicographical order of the keys.
pub(crate) const MESSAGE_PADDING_BYTES: [u8; 2] = [0xFF, 0xFF];

// For messages we do not use asset id. These bytes are only used as a placeholder
// to maintain the correct, lexicographical order of the keys.
pub(crate) const ASSET_ID_FOR_MESSAGES: [u8; 32] = [0x00; 32];

fn add_coin<T>(block_st_transaction: &mut T, coin: &Coin) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = CoinsToSpendIndexKey::from_coin(coin);
    let storage = block_st_transaction.storage::<CoinsToSpendIndex>();
    let maybe_old_value = storage.replace(&key, &())?;
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

// fn add_message<T>(
// block_st_transaction: &mut T,
// message: &Message,
// ) -> Result<(), IndexationError>
// where
// T: OffChainDatabaseTransaction,
// {
// let key = CoinsToSpendIndexKey::from_message(message);
// let storage = block_st_transaction.storage::<CoinsToSpendIndex>();
// let maybe_old_value = storage.replace(&key, &())?;
// if maybe_old_value.is_some() {
// return Err(IndexationError::CoinToSpendAlreadyIndexed {
// owner: coin.owner.clone(),
// asset_id: coin.asset_id.clone(),
// amount: coin.amount,
// utxo_id: coin.utxo_id.clone(),
// });
// }
// Ok(())
// }

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
        Event::MessageImported(message) => todo!(), /* add_message(block_st_transaction, message), */
        Event::MessageConsumed(message) => todo!(),
        Event::CoinCreated(coin) => add_coin(block_st_transaction, coin),
        Event::CoinConsumed(coin) => remove_coin(block_st_transaction, coin),
        Event::ForcedTransactionFailed { .. } => todo!(),
    };

    Ok(())
}
