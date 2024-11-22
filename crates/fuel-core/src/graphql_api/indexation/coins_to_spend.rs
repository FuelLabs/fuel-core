use fuel_core_types::services::executor::Event;

use crate::graphql_api::ports::worker::OffChainDatabaseTransaction;

use super::IndexationError;

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
        Event::MessageImported(message) => (),
        Event::MessageConsumed(message) => (),
        Event::CoinCreated(coin) => (),
        Event::CoinConsumed(coin) => (),
        Event::ForcedTransactionFailed { .. } => (),
    };

    Ok(())
}
