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
    // TODO[RC]: Add tests
}
