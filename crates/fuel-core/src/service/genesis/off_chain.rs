use crate::{
    database::{
        database_description::off_chain::OffChain,
        Database,
    },
    graphql_api::worker_service,
    service::Config,
};
use fuel_core_chain_config::{
    CoinConfig,
    MessageConfig,
};
use fuel_core_storage::transactional::Transactional;
use fuel_core_types::{
    entities::coins::coin::Coin,
    services::executor::Event,
};
use std::borrow::Cow;

fn process_message_batch(
    db: &Database<OffChain>,
    messages: Vec<MessageConfig>,
) -> anyhow::Result<()> {
    let mut database_transaction = Transactional::transaction(db);

    let message_events = messages.iter().map(|config| {
        let message = config.clone().into();
        Cow::Owned(Event::MessageImported(message))
    });

    worker_service::Task::process_executor_events(
        message_events,
        database_transaction.as_mut(),
    )?;

    database_transaction.commit()?;
    Ok(())
}

fn process_coin_batch(
    db: &Database<OffChain>,
    coins: Vec<CoinConfig>,
    _output_index: &mut u64,
) -> anyhow::Result<()> {
    let mut database_transaction = Transactional::transaction(db);

    let coin_events = coins.iter().map(|config| {
        let coin = Coin {
            utxo_id: config.utxo_id(),
            owner: config.owner,
            amount: config.amount,
            asset_id: config.asset_id,
            maturity: config.maturity,
            tx_pointer: config.tx_pointer(),
        };
        Cow::Owned(Event::CoinCreated(coin))
    });

    worker_service::Task::process_executor_events(
        coin_events,
        database_transaction.as_mut(),
    )?;

    database_transaction.commit()?;
    Ok(())
}

/// Performs the importing of the genesis block from the snapshot.
pub fn execute_genesis_block(
    config: &Config,
    original_database: &Database<OffChain>,
) -> anyhow::Result<()> {
    for message_group in config.state_reader.messages()? {
        process_message_batch(original_database, message_group?.data)?;
    }

    let mut output_index = 0;
    for coin_group in config.state_reader.coins()? {
        process_coin_batch(original_database, coin_group?.data, &mut output_index)?;
    }

    Ok(())
}
