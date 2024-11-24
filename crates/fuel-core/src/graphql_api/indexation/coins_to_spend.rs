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

fn register_new_coin_to_spend<T>(
    block_st_transaction: &mut T,
    coin: &Coin,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = CoinsToSpendIndexKey::from_coin(coin);
    let storage = block_st_transaction.storage::<CoinsToSpendIndex>();
    Ok(storage.insert(&key, &())?)
}

fn register_coin_spent<T>(
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
        Event::MessageImported(message) => todo!(),
        Event::MessageConsumed(message) => todo!(),
        Event::CoinCreated(coin) => {
            register_new_coin_to_spend(block_st_transaction, coin)
        }
        Event::CoinConsumed(coin) => register_coin_spent(block_st_transaction, coin),
        Event::ForcedTransactionFailed { .. } => todo!(),
    };

    Ok(())
}
