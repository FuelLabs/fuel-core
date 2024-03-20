use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
        },
        Database,
    },
    graphql_api::worker_service,
};
use fuel_core_storage::{
    iter::IteratorOverTable,
    tables::{
        Coins,
        Messages,
        Transactions,
    },
    transactional::WriteTransaction,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    entities::{
        coins::coin::CompressedCoin,
        message::Message,
    },
    fuel_tx::{
        Transaction,
        UtxoId,
    },
    fuel_types::Nonce,
    services::executor::Event,
};
use itertools::Itertools;
use std::borrow::Cow;

fn process_messages(
    original_database: &mut Database<OffChain>,
    messages: Vec<(Nonce, Message)>,
) -> anyhow::Result<()> {
    let mut database_transaction = original_database.write_transaction();

    let message_events = messages
        .into_iter()
        .map(|(_, message)| Cow::Owned(Event::MessageImported(message)));

    worker_service::process_executor_events(message_events, &mut database_transaction)?;

    database_transaction.commit()?;
    Ok(())
}

fn process_coins(
    original_database: &mut Database<OffChain>,
    coins: Vec<(UtxoId, CompressedCoin)>,
) -> anyhow::Result<()> {
    let mut database_transaction = original_database.write_transaction();

    let coin_events = coins
        .into_iter()
        .map(|(utxo_id, coin)| Cow::Owned(Event::CoinCreated(coin.uncompress(utxo_id))));

    worker_service::process_executor_events(coin_events, &mut database_transaction)?;

    database_transaction.commit()?;
    Ok(())
}

fn process_transactions(
    original_database: &mut Database<OffChain>,
    transactions: Vec<(TxId, Transaction)>,
) -> anyhow::Result<()> {
    let mut database_transaction = original_database.write_transaction();

    let transactions = transactions.iter().map(|(_, tx)| tx);

    worker_service::process_transactions(transactions, &mut database_transaction)?;

    database_transaction.commit()?;
    Ok(())
}

/// Performs the importing of the genesis block from the snapshot.
// TODO: The regenesis of the off-chain database should go in the same way as the on-chain database.
//  https://github.com/FuelLabs/fuel-core/issues/1619
pub fn execute_genesis_block(
    on_chain_database: &Database<OnChain>,
    off_chain_database: &mut Database<OffChain>,
) -> anyhow::Result<()> {
    for chunk in on_chain_database
        .iter_all::<Messages>(None)
        .chunks(1000)
        .into_iter()
    {
        let chunk: Vec<_> = chunk.try_collect()?;
        process_messages(off_chain_database, chunk)?;
    }

    for chunk in on_chain_database
        .iter_all::<Coins>(None)
        .chunks(1000)
        .into_iter()
    {
        let chunk: Vec<_> = chunk.try_collect()?;
        process_coins(off_chain_database, chunk)?;
    }

    for chunk in on_chain_database
        .iter_all::<Transactions>(None)
        .chunks(1000)
        .into_iter()
    {
        let chunk: Vec<_> = chunk.try_collect()?;
        process_transactions(off_chain_database, chunk)?;
    }

    Ok(())
}
