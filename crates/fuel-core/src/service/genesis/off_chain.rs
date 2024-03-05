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
    fuel_tx::{
        Bytes32,
        UtxoId,
    },
    fuel_types::bytes::WORD_SIZE,
    services::executor::Event,
};
use itertools::Itertools;
use std::borrow::Cow;

fn process_messages(
    db: &Database<OffChain>,
    messages: Vec<MessageConfig>,
) -> anyhow::Result<()> {
    let mut database_transaction = Transactional::transaction(db);

    let message_events = messages.iter().map(|config| {
        let message = config.clone().into();
        Cow::Owned(Event::MessageImported(message))
    });

    worker_service::process_executor_events(
        message_events,
        database_transaction.as_mut(),
    )?;

    database_transaction.commit()?;
    Ok(())
}

// TODO: Remove as part of the https://github.com/FuelLabs/fuel-core/issues/1668
fn generated_utxo_id(output_index: u64) -> UtxoId {
    UtxoId::new(
        // generated transaction id([0..[out_index/255]])
        Bytes32::try_from(
            (0..(Bytes32::LEN - WORD_SIZE))
                .map(|_| 0u8)
                .chain((output_index / 255).to_be_bytes())
                .collect_vec()
                .as_slice(),
        )
        .expect("Incorrect genesis transaction id byte length"),
        (output_index % 255) as u8,
    )
}

fn process_coins(
    db: &Database<OffChain>,
    coins: Vec<CoinConfig>,
    output_index: &mut u64,
) -> anyhow::Result<()> {
    let mut database_transaction = Transactional::transaction(db);

    let coin_events = coins.iter().map(|config| {
        let utxo_id = config.utxo_id().unwrap_or(generated_utxo_id(*output_index));

        *output_index = output_index
                .checked_add(1)
                .expect("The maximum number of UTXOs supported in the genesis configuration has been exceeded.");

        let coin = Coin {
            utxo_id,
            owner: config.owner,
            amount: config.amount,
            asset_id: config.asset_id,
            tx_pointer: config.tx_pointer(),
        };
        Cow::Owned(Event::CoinCreated(coin))
    });

    worker_service::process_executor_events(coin_events, database_transaction.as_mut())?;

    database_transaction.commit()?;
    Ok(())
}

/// Performs the importing of the genesis block from the snapshot.
// TODO: The regenesis of the off-chain database should go in the same way as the on-chain database.
//  https://github.com/FuelLabs/fuel-core/issues/1619
pub fn execute_genesis_block(
    config: &Config,
    original_database: &Database<OffChain>,
) -> anyhow::Result<()> {
    for message_group in config.state_reader.messages()? {
        process_messages(original_database, message_group?.data)?;
    }

    let mut output_index = 0;
    for coin_group in config.state_reader.coins()? {
        process_coins(original_database, coin_group?.data, &mut output_index)?;
    }

    Ok(())
}
