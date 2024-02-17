use crate::{
    database::{
        database_description::off_chain::OffChain,
        Database,
    },
    graphql_api::worker_service,
    service::{
        genesis::create_coin_from_config,
        Config,
    },
};
use fuel_core_storage::transactional::{
    StorageTransaction,
    Transactional,
};
use fuel_core_types::{
    entities::message::Message,
    services::executor::Event,
};
use std::borrow::Cow;

/// Performs the importing of the genesis block from the snapshot.
pub fn execute_genesis_block(
    config: &Config,
    original_database: &Database<OffChain>,
) -> anyhow::Result<StorageTransaction<Database<OffChain>>> {
    // start a db transaction for bulk-writing
    let mut database_transaction = Transactional::transaction(original_database);

    if let Some(state_config) = &config.chain_conf.initial_state {
        if let Some(messages) = &state_config.messages {
            let messages_events = messages.iter().map(|config| {
                let message: Message = config.clone().into();
                Cow::Owned(Event::MessageImported(message))
            });

            worker_service::Task::process_executor_events(
                messages_events,
                database_transaction.as_mut(),
            )?;
        }

        if let Some(coins) = &state_config.coins {
            let mut generated_output_index = 0;
            let coin_events = coins.iter().map(|config| {
                let coin = create_coin_from_config(config, &mut generated_output_index);
                Cow::Owned(Event::CoinCreated(coin))
            });

            worker_service::Task::process_executor_events(
                coin_events,
                database_transaction.as_mut(),
            )?;
        }
    }

    Ok(database_transaction)
}
