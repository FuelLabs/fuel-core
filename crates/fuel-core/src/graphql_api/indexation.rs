use fuel_core_storage::{
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageInspect,
    StorageMutate,
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

#[derive(derive_more::From, derive_more::Display)]
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

// TODO[RC]: A lot of duplication below, consider refactoring.
fn increase_message_balance<T>(
    block_st_transaction: &mut T,
    message: &Message,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let key = message.recipient();
    let storage = block_st_transaction.storage::<MessageBalances>();
    let MessageBalance {
        mut retryable,
        mut non_retryable,
    } = *storage.get(&key)?.unwrap_or_default();

    if message.has_retryable_amount() {
        retryable.saturating_add(message.amount() as u128);
    } else {
        non_retryable.saturating_add(message.amount() as u128);
    }

    let new_balance = MessageBalance {
        retryable,
        non_retryable,
    };

    let storage = block_st_transaction.storage::<MessageBalances>();
    Ok(storage.insert(&key, &new_balance)?)
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
        mut retryable,
        mut non_retryable,
    } = *storage.get(&key)?.unwrap_or_default();

    if message.has_retryable_amount() {
        let maybe_new_amount = retryable.checked_sub(message.amount() as u128);
        match maybe_new_amount {
            Some(new_amount) => {
                let storage = block_st_transaction.storage::<MessageBalances>();
                let new_balance = MessageBalance {
                    retryable: new_amount,
                    non_retryable,
                };
                return Ok(storage.insert(&key, &new_balance)?);
            }
            None => {
                return Err(IndexationError::MessageBalanceWouldUnderflow {
                    owner: message.recipient().clone(),
                    current_amount: retryable,
                    requested_deduction: message.amount() as u128,
                    retryable: true,
                });
            }
        }
    } else {
        let maybe_new_amount = non_retryable.checked_sub(message.amount() as u128);
        match maybe_new_amount {
            Some(new_amount) => {
                let storage = block_st_transaction.storage::<MessageBalances>();
                let new_balance = MessageBalance {
                    retryable: new_amount,
                    non_retryable,
                };
                return Ok(storage.insert(&key, &new_balance)?);
            }
            None => {
                return Err(IndexationError::MessageBalanceWouldUnderflow {
                    owner: message.recipient().clone(),
                    current_amount: retryable,
                    requested_deduction: message.amount() as u128,
                    retryable: false,
                });
            }
        }
    }
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
    let mut amount = *storage.get(&key)?.unwrap_or_default();
    amount.saturating_add(coin.amount as u128);

    let storage = block_st_transaction.storage::<CoinBalances>();
    Ok(storage.insert(&key, &amount)?)
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
    let mut current_amount = *storage.get(&key)?.unwrap_or_default();

    let maybe_new_amount = current_amount.checked_sub(coin.amount as u128);
    match maybe_new_amount {
        Some(new_amount) => {
            let storage = block_st_transaction.storage::<CoinBalances>();
            Ok(storage.insert(&key, &new_amount)?)
        }
        None => Err(IndexationError::CoinBalanceWouldUnderflow {
            owner: coin.owner.clone(),
            asset_id: coin.asset_id.clone(),
            current_amount,
            requested_deduction: coin.amount as u128,
        }),
    }
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
        Event::ForcedTransactionFailed {
            id,
            block_height,
            failure,
        } => Ok(()),
    }
}
