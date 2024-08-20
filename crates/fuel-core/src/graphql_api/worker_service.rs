use super::storage::old::{
    OldFuelBlockConsensus,
    OldFuelBlocks,
    OldTransactions,
};
use crate::{
    fuel_core_graphql_api::{
        ports,
        ports::worker::OffChainDatabaseTransaction,
        storage::{
            blocks::FuelBlockIdsToHeights,
            coins::{
                owner_coin_id_key,
                OwnedCoins,
            },
            contracts::ContractsInfo,
            messages::{
                OwnedMessageIds,
                OwnedMessageKey,
                SpentMessages,
            },
        },
    },
    graphql_api::storage::relayed_transactions::RelayedTransactionStatuses,
};
use fuel_core_metrics::graphql_metrics::graphql_metrics;
use fuel_core_services::{
    stream::BoxStream,
    EmptyShared,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_storage::{
    Error as StorageError,
    Result as StorageResult,
    StorageAsMut,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            CompressedBlock,
        },
        consensus::Consensus,
    },
    entities::relayer::transaction::RelayedTransactionStatus,
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
            Salt,
            StorageSlots,
        },
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
        Contract,
        Input,
        Output,
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
        ChainId,
    },
    services::{
        block_importer::{
            ImportResult,
            SharedImportResult,
        },
        executor::{
            Event,
            TransactionExecutionStatus,
        },
        txpool::from_executor_to_status,
    },
};
use futures::{
    FutureExt,
    StreamExt,
};
use std::{
    borrow::Cow,
    ops::Deref,
};

#[cfg(test)]
mod tests;

/// The initialization task recovers the state of the GraphQL service database on startup.
pub struct InitializeTask<TxPool, BlockImporter, OnChain, OffChain> {
    chain_id: ChainId,
    continue_on_error: bool,
    tx_pool: TxPool,
    blocks_events: BoxStream<SharedImportResult>,
    block_importer: BlockImporter,
    on_chain_database: OnChain,
    off_chain_database: OffChain,
}

/// The off-chain GraphQL API worker task processes the imported blocks
/// and actualize the information used by the GraphQL service.
pub struct Task<TxPool, D> {
    tx_pool: TxPool,
    block_importer: BoxStream<SharedImportResult>,
    database: D,
    chain_id: ChainId,
    continue_on_error: bool,
}

impl<TxPool, D> Task<TxPool, D>
where
    TxPool: ports::worker::TxPool,
    D: ports::worker::OffChainDatabase,
{
    fn process_block(&mut self, result: SharedImportResult) -> anyhow::Result<()> {
        let block = &result.sealed_block.entity;
        let mut transaction = self.database.transaction();
        // save the status for every transaction using the finalized block id
        persist_transaction_status(&result, &mut transaction)?;

        // save the associated owner for each transaction in the block
        index_tx_owners_for_block(block, &mut transaction, &self.chain_id)?;

        // save the transaction related information
        process_transactions(block.transactions().iter(), &mut transaction)?;

        let height = block.header().height();
        let block_id = block.id();
        transaction
            .storage::<FuelBlockIdsToHeights>()
            .insert(&block_id, height)?;

        let total_tx_count = transaction
            .increase_tx_count(block.transactions().len() as u64)
            .unwrap_or_default();

        process_executor_events(
            result.events.iter().map(Cow::Borrowed),
            &mut transaction,
        )?;

        transaction.commit()?;

        for status in result.tx_status.iter() {
            let tx_id = status.id;
            let status = from_executor_to_status(block, status.result.clone());
            self.tx_pool.send_complete(tx_id, height, status);
        }

        // update the importer metrics after the block is successfully committed
        graphql_metrics().total_txs_count.set(total_tx_count as i64);

        Ok(())
    }
}

/// Process the executor events and update the indexes for the messages and coins.
pub fn process_executor_events<'a, Iter, T>(
    events: Iter,
    block_st_transaction: &mut T,
) -> anyhow::Result<()>
where
    Iter: Iterator<Item = Cow<'a, Event>>,
    T: OffChainDatabaseTransaction,
{
    for event in events {
        match event.deref() {
            Event::MessageImported(message) => {
                block_st_transaction
                    .storage_as_mut::<OwnedMessageIds>()
                    .insert(
                        &OwnedMessageKey::new(message.recipient(), message.nonce()),
                        &(),
                    )?;
            }
            Event::MessageConsumed(message) => {
                block_st_transaction
                    .storage_as_mut::<OwnedMessageIds>()
                    .remove(&OwnedMessageKey::new(
                        message.recipient(),
                        message.nonce(),
                    ))?;
                block_st_transaction
                    .storage::<SpentMessages>()
                    .insert(message.nonce(), &())?;
            }
            Event::CoinCreated(coin) => {
                let coin_by_owner = owner_coin_id_key(&coin.owner, &coin.utxo_id);
                block_st_transaction
                    .storage_as_mut::<OwnedCoins>()
                    .insert(&coin_by_owner, &())?;
            }
            Event::CoinConsumed(coin) => {
                let key = owner_coin_id_key(&coin.owner, &coin.utxo_id);
                block_st_transaction
                    .storage_as_mut::<OwnedCoins>()
                    .remove(&key)?;
            }
            Event::ForcedTransactionFailed {
                id,
                block_height,
                failure,
            } => {
                let status = RelayedTransactionStatus::Failed {
                    block_height: *block_height,
                    failure: failure.clone(),
                };

                block_st_transaction
                    .storage_as_mut::<RelayedTransactionStatuses>()
                    .insert(&Bytes32::from(id.to_owned()), &status)?;
            }
        }
    }
    Ok(())
}

/// Associate all transactions within a block to their respective UTXO owners
fn index_tx_owners_for_block<T>(
    block: &Block,
    block_st_transaction: &mut T,
    chain_id: &ChainId,
) -> anyhow::Result<()>
where
    T: OffChainDatabaseTransaction,
{
    for (tx_idx, tx) in block.transactions().iter().enumerate() {
        let block_height = *block.header().height();
        let inputs;
        let outputs;
        let tx_idx = u16::try_from(tx_idx).map_err(|e| {
            anyhow::anyhow!("The block has more than `u16::MAX` transactions, {}", e)
        })?;
        let tx_id = tx.id(chain_id);
        match tx {
            Transaction::Script(tx) => {
                inputs = tx.inputs().as_slice();
                outputs = tx.outputs().as_slice();
            }
            Transaction::Create(tx) => {
                inputs = tx.inputs().as_slice();
                outputs = tx.outputs().as_slice();
            }
            Transaction::Mint(_) => continue,
            Transaction::Upgrade(tx) => {
                inputs = tx.inputs().as_slice();
                outputs = tx.outputs().as_slice();
            }
            Transaction::Upload(tx) => {
                inputs = tx.inputs().as_slice();
                outputs = tx.outputs().as_slice();
            }
            Transaction::Blob(tx) => {
                inputs = tx.inputs().as_slice();
                outputs = tx.outputs().as_slice();
            }
        }
        persist_owners_index(
            block_height,
            inputs,
            outputs,
            &tx_id,
            tx_idx,
            block_st_transaction,
        )?;
    }
    Ok(())
}

/// Index the tx id by owner for all of the inputs and outputs
fn persist_owners_index<T>(
    block_height: BlockHeight,
    inputs: &[Input],
    outputs: &[Output],
    tx_id: &Bytes32,
    tx_idx: u16,
    db: &mut T,
) -> StorageResult<()>
where
    T: OffChainDatabaseTransaction,
{
    let mut owners = vec![];
    for input in inputs {
        if let Input::CoinSigned(CoinSigned { owner, .. })
        | Input::CoinPredicate(CoinPredicate { owner, .. }) = input
        {
            owners.push(owner);
        }
    }

    for output in outputs {
        match output {
            Output::Coin { to, .. }
            | Output::Change { to, .. }
            | Output::Variable { to, .. } => {
                owners.push(to);
            }
            Output::Contract(_) | Output::ContractCreated { .. } => {}
        }
    }

    // dedupe owners from inputs and outputs prior to indexing
    owners.sort();
    owners.dedup();

    for owner in owners {
        db.record_tx_id_owner(owner, block_height, tx_idx, tx_id)?;
    }

    Ok(())
}

fn persist_transaction_status<T>(
    import_result: &ImportResult,
    db: &mut T,
) -> StorageResult<()>
where
    T: OffChainDatabaseTransaction,
{
    for TransactionExecutionStatus { id, result } in import_result.tx_status.iter() {
        let status =
            from_executor_to_status(&import_result.sealed_block.entity, result.clone());

        if db.update_tx_status(id, status)?.is_some() {
            return Err(anyhow::anyhow!(
                "Transaction status already exists for tx {}",
                id
            )
            .into());
        }
    }
    Ok(())
}

pub fn process_transactions<'a, I, T>(transactions: I, db: &mut T) -> StorageResult<()>
where
    I: Iterator<Item = &'a Transaction>,
    T: OffChainDatabaseTransaction,
{
    for tx in transactions {
        match tx {
            Transaction::Create(tx) => {
                let contract_id = tx
                    .outputs()
                    .iter()
                    .filter_map(|output| output.contract_id().cloned())
                    .next()
                    .map(Ok::<_, StorageError>)
                    .unwrap_or_else(|| {
                        // TODO: Reuse `CreateMetadata` when it will be exported
                        //  from the `fuel-tx` crate.
                        let salt = tx.salt();
                        let storage_slots = tx.storage_slots();
                        let contract = Contract::try_from(tx)
                            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                        let contract_root = contract.root();
                        let state_root =
                            Contract::initial_state_root(storage_slots.iter());
                        Ok::<_, StorageError>(contract.id(
                            salt,
                            &contract_root,
                            &state_root,
                        ))
                    })?;

                let salt = *tx.salt();

                db.storage::<ContractsInfo>()
                    .insert(&contract_id, &(salt.into()))?;
            }
            Transaction::Script(_)
            | Transaction::Mint(_)
            | Transaction::Upgrade(_)
            | Transaction::Upload(_)
            | Transaction::Blob(_) => {
                // Do nothing
            }
        }
    }
    Ok(())
}

pub fn copy_to_old_blocks<'a, I, T>(blocks: I, db: &mut T) -> StorageResult<()>
where
    I: Iterator<Item = (&'a BlockHeight, &'a CompressedBlock)>,
    T: OffChainDatabaseTransaction,
{
    for (height, block) in blocks {
        db.storage::<OldFuelBlocks>().insert(height, block)?;
    }
    Ok(())
}

pub fn copy_to_old_block_consensus<'a, I, T>(blocks: I, db: &mut T) -> StorageResult<()>
where
    I: Iterator<Item = (&'a BlockHeight, &'a Consensus)>,
    T: OffChainDatabaseTransaction,
{
    for (height, block) in blocks {
        db.storage::<OldFuelBlockConsensus>()
            .insert(height, block)?;
    }
    Ok(())
}

pub fn copy_to_old_transactions<'a, I, T>(
    transactions: I,
    db: &mut T,
) -> StorageResult<()>
where
    I: Iterator<Item = (&'a TxId, &'a Transaction)>,
    T: OffChainDatabaseTransaction,
{
    for (id, tx) in transactions {
        db.storage::<OldTransactions>().insert(id, tx)?;
    }
    Ok(())
}

#[async_trait::async_trait]
impl<TxPool, BlockImporter, OnChain, OffChain> RunnableService
    for InitializeTask<TxPool, BlockImporter, OnChain, OffChain>
where
    TxPool: ports::worker::TxPool,
    BlockImporter: ports::worker::BlockImporter,
    OnChain: ports::worker::OnChainDatabase,
    OffChain: ports::worker::OffChainDatabase,
{
    const NAME: &'static str = "GraphQL_Off_Chain_Worker";
    type SharedData = EmptyShared;
    type Task = Task<TxPool, OffChain>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        EmptyShared
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        {
            let db_tx = self.off_chain_database.transaction();
            let total_tx_count = db_tx.get_tx_count().unwrap_or_default();
            graphql_metrics().total_txs_count.set(total_tx_count as i64);
        }

        let InitializeTask {
            chain_id,
            tx_pool,
            block_importer,
            blocks_events,
            on_chain_database,
            off_chain_database,
            continue_on_error,
        } = self;

        let mut task = Task {
            tx_pool,
            block_importer: blocks_events,
            database: off_chain_database,
            chain_id,
            continue_on_error,
        };

        let mut target_chain_height = on_chain_database.latest_height()?;
        // Process all blocks that were imported before the service started.
        // The block importer may produce some blocks on start-up during the
        // genesis stage or the recovery process. In this case, we need to
        // process these blocks because, without them,
        // our block height will be less than on the chain database.
        while let Some(Some(block)) = task.block_importer.next().now_or_never() {
            target_chain_height = Some(*block.sealed_block.entity.header().height());
            task.process_block(block)?;
        }

        sync_databases(&mut task, target_chain_height, &block_importer)?;

        Ok(task)
    }
}

fn sync_databases<TxPool, BlockImporter, OffChain>(
    task: &mut Task<TxPool, OffChain>,
    target_chain_height: Option<BlockHeight>,
    import_result_provider: &BlockImporter,
) -> anyhow::Result<()>
where
    TxPool: ports::worker::TxPool,
    BlockImporter: ports::worker::BlockImporter,
    OffChain: ports::worker::OffChainDatabase,
{
    loop {
        let off_chain_height = task.database.latest_height()?;

        if target_chain_height < off_chain_height {
            return Err(anyhow::anyhow!(
                "The target chain height is lower than the off-chain database height"
            ));
        }

        if target_chain_height == off_chain_height {
            break;
        }

        let next_block_height =
            off_chain_height.map(|height| BlockHeight::new(height.saturating_add(1)));

        let import_result =
            import_result_provider.block_event_at_height(next_block_height)?;

        task.process_block(import_result)?
    }

    Ok(())
}

#[async_trait::async_trait]
impl<TxPool, D> RunnableTask for Task<TxPool, D>
where
    TxPool: ports::worker::TxPool,
    D: ports::worker::OffChainDatabase,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                should_continue = false;
            }

            result = self.block_importer.next() => {
                if let Some(block) = result {
                    let result = self.process_block(block);

                    // In the case of an error, shut down the service to avoid a huge
                    // de-synchronization between on-chain and off-chain databases.
                    if let Err(e) = result {
                        tracing::error!("Error processing block: {:?}", e);
                        should_continue = self.continue_on_error;
                    } else {
                        should_continue = true
                    }
                } else {
                    should_continue = false
                }
            }
        }
        Ok(should_continue)
    }

    async fn shutdown(mut self) -> anyhow::Result<()> {
        // Process all remaining blocks before shutdown to not lose any data.
        loop {
            let result = self.block_importer.next().now_or_never();

            if let Some(Some(block)) = result {
                self.process_block(block)?;
            } else {
                break;
            }
        }
        Ok(())
    }
}

pub fn new_service<TxPool, BlockImporter, OnChain, OffChain>(
    tx_pool: TxPool,
    block_importer: BlockImporter,
    on_chain_database: OnChain,
    off_chain_database: OffChain,
    chain_id: ChainId,
    continue_on_error: bool,
) -> ServiceRunner<InitializeTask<TxPool, BlockImporter, OnChain, OffChain>>
where
    TxPool: ports::worker::TxPool,
    OnChain: ports::worker::OnChainDatabase,
    OffChain: ports::worker::OffChainDatabase,
    BlockImporter: ports::worker::BlockImporter,
{
    ServiceRunner::new(InitializeTask {
        tx_pool,
        blocks_events: block_importer.block_events(),
        block_importer,
        on_chain_database,
        off_chain_database,
        chain_id,
        continue_on_error,
    })
}
