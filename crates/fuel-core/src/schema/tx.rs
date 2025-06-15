use super::scalars::{
    AssetId,
    U16,
    U32,
    U64,
};
use crate::{
    coins_query::CoinsQueryError,
    fuel_core_graphql_api::{
        Config as GraphQLConfig,
        IntoApiResult,
        api_service::{
            BlockProducer,
            ChainInfoProvider,
            DynTxStatusManager,
            TxPool,
        },
        query_costs,
    },
    graphql_api::{
        database::ReadView,
        ports::MemoryPool,
    },
    query::{
        TxnStatusChangeState,
        asset_query::Exclude,
        transaction_status_change,
    },
    schema::{
        ReadViewProvider,
        coins::ExcludeInput,
        gas_price::EstimateGasPriceExt,
        scalars::{
            Address,
            HexString,
            SortedTxCursor,
            TransactionId,
            TxPointer,
        },
        tx::{
            assemble_tx::{
                AssembleArguments,
                AssembleTx,
            },
            types::{
                AssembleTransactionResult,
                TransactionStatus,
                get_tx_status,
            },
        },
    },
    service::adapters::SharedMemoryPool,
};
use async_graphql::{
    Context,
    Object,
    Subscription,
    connection::{
        Connection,
        EmptyFields,
    },
};
use fuel_core_storage::{
    Error as StorageError,
    IsNotFound,
    PredicateStorageRequirements,
    Result as StorageResult,
    iter::IterDirection,
};
use fuel_core_tx_status_manager::TxStatusMessage;
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_tx::{
        self,
        Bytes32,
        Cacheable,
        Transaction as FuelTx,
        UniqueIdentifier,
    },
    fuel_types::{
        self,
        canonical::Deserialize,
    },
    fuel_vm::checked_transaction::{
        CheckPredicateParams,
        EstimatePredicates,
    },
    services::{
        executor::DryRunResult,
        transaction_status,
    },
};
use futures::{
    Stream,
    TryStreamExt,
};
use std::{
    borrow::Cow,
    future::Future,
    iter,
    sync::Arc,
};
use types::{
    DryRunStorageReads,
    DryRunTransactionExecutionStatus,
    StorageReadReplayEvent,
    Transaction,
};

mod assemble_tx;
pub mod input;
pub mod output;
pub mod receipt;
pub mod types;
pub mod upgrade_purpose;

#[derive(Default)]
pub struct TxQuery;

impl TxQuery {
    /// The actual logic of all different dry-run queries.
    async fn dry_run_inner(
        &self,
        ctx: &Context<'_>,
        txs: Vec<HexString<'_>>,
        // If set to false, disable input utxo validation, overriding the configuration of the node.
        // This allows for non-existent inputs to be used without signature validation
        // for read-only calls.
        utxo_validation: Option<bool>,
        gas_price: Option<U64>,
        // This can be used to run the dry-run on top of a past block.
        // Requires `--historical-execution` flag to be enabled.
        block_height: Option<U32>,
        // Record storage reads, so this tx can be used with execution tracer in a local debugger.
        record_storage_reads: bool,
    ) -> async_graphql::Result<DryRunStorageReads> {
        let config = ctx.data_unchecked::<GraphQLConfig>().clone();
        let block_producer = ctx.data_unchecked::<BlockProducer>();
        let consensus_params = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();
        let block_gas_limit = consensus_params.block_gas_limit();

        if block_height.is_some() && !config.historical_execution {
            return Err(anyhow::anyhow!(
                "The `blockHeight` parameter requires the `--historical-execution` option"
            )
            .into());
        }

        let mut transactions = txs
            .iter()
            .map(|tx| FuelTx::from_bytes(&tx.0))
            .collect::<Result<Vec<FuelTx>, _>>()?;
        transactions.iter_mut().try_fold::<_, _, async_graphql::Result<u64>>(0u64, |acc, tx| {
            let gas = tx.max_gas(&consensus_params)?;
            let gas = gas.saturating_add(acc);
            if gas > block_gas_limit {
                return Err(anyhow::anyhow!("The sum of the gas usable by the transactions is greater than the block gas limit").into());
            }
            tx.precompute(&consensus_params.chain_id())?;
            Ok(gas)
        })?;

        let DryRunResult {
            transactions,
            storage_reads,
        } = block_producer
            .dry_run_txs(
                transactions,
                block_height.map(|x| x.into()),
                None, // TODO(#1749): Pass parameter from API
                utxo_validation,
                gas_price.map(|x| x.into()),
                record_storage_reads,
            )
            .await?;

        let tx_statuses = transactions
            .into_iter()
            .map(|(_, status)| DryRunTransactionExecutionStatus(status))
            .collect();

        let storage_reads = storage_reads
            .into_iter()
            .map(|event| event.into())
            .collect();

        Ok(DryRunStorageReads {
            tx_statuses,
            storage_reads,
        })
    }
}

#[Object]
impl TxQuery {
    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn transaction(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The ID of the transaction")] id: TransactionId,
    ) -> async_graphql::Result<Option<Transaction>> {
        let query = ctx.read_view()?;
        let id = id.0;
        let txpool = ctx.data_unchecked::<TxPool>();

        match txpool.transaction(id).await? {
            Some(transaction) => Ok(Some(Transaction(transaction, id))),
            _ => query
                .transaction(&id)
                .map(|tx| Transaction::from_tx(id, tx))
                .into_api_result(),
        }
    }

    // We assume that each block has 100 transactions.
    #[graphql(complexity = "{\
        (query_costs().tx_get + child_complexity) \
        * (first.unwrap_or_default() as usize + last.unwrap_or_default() as usize)
    }")]
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<
        Connection<SortedTxCursor, Transaction, EmptyFields, EmptyFields>,
    > {
        use futures::stream::StreamExt;
        let query = ctx.read_view()?;
        let query_ref = query.as_ref();
        crate::schema::query_pagination(
            after,
            before,
            first,
            last,
            |start: &Option<SortedTxCursor>, direction| {
                let start = *start;
                let block_id = start.map(|sorted| sorted.block_height);
                let compressed_blocks = query.compressed_blocks(block_id, direction);

                let all_txs = compressed_blocks
                    .map_ok(move |fuel_block| {
                        let (header, mut txs) = fuel_block.into_inner();

                        if direction == IterDirection::Reverse {
                            txs.reverse();
                        }

                        let iter = txs.into_iter().zip(iter::repeat(*header.height()));
                        futures::stream::iter(iter).map(Ok)
                    })
                    .try_flatten()
                    .map_ok(|(tx_id, block_height)| {
                        SortedTxCursor::new(block_height, tx_id.into())
                    })
                    .try_skip_while(move |sorted| {
                        let skip = if let Some(start) = start {
                            sorted != &start
                        } else {
                            false
                        };

                        async move { Ok::<_, StorageError>(skip) }
                    })
                    .chunks(query_ref.batch_size)
                    .map(|chunk| {
                        use itertools::Itertools;

                        let chunk = chunk.into_iter().try_collect::<_, Vec<_>, _>()?;
                        Ok::<_, StorageError>(chunk)
                    })
                    .try_filter_map(move |chunk| {
                        let async_query = query_ref.clone();
                        async move {
                            let tx_ids = chunk
                                .iter()
                                .map(|sorted| sorted.tx_id.0)
                                .collect::<Vec<_>>();
                            let txs = async_query.transactions(tx_ids).await;
                            let txs = txs.into_iter().zip(chunk.into_iter()).map(
                                |(result, sorted)| {
                                    result.map(|tx| {
                                        (sorted, Transaction::from_tx(sorted.tx_id.0, tx))
                                    })
                                },
                            );
                            Ok(Some(futures::stream::iter(txs)))
                        }
                    })
                    .try_flatten();

                Ok(all_txs)
            },
        )
        .await
    }

    #[graphql(complexity = "{\
        query_costs().storage_iterator\
        + (query_costs().storage_read + first.unwrap_or_default() as usize) * child_complexity \
        + (query_costs().storage_read + last.unwrap_or_default() as usize) * child_complexity\
    }")]
    async fn transactions_by_owner(
        &self,
        ctx: &Context<'_>,
        owner: Address,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<TxPointer, Transaction, EmptyFields, EmptyFields>>
    {
        use futures::stream::StreamExt;
        let query = ctx.read_view()?;
        let params = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();
        let owner = fuel_types::Address::from(owner);

        crate::schema::query_pagination(
            after,
            before,
            first,
            last,
            |start: &Option<TxPointer>, direction| {
                let start = (*start).map(Into::into);
                let txs =
                    query
                        .owned_transactions(owner, start, direction)
                        .map(|result| {
                            result.map(|(cursor, tx)| {
                                let tx_id = tx.id(&params.chain_id());
                                (cursor.into(), Transaction::from_tx(tx_id, tx))
                            })
                        });
                Ok(txs)
            },
        )
        .await
    }

    /// Assembles the transaction based on the provided requirements.
    /// The return transaction contains:
    /// - Input coins to cover `required_balances`
    /// - Input coins to cover the fee of the transaction based on the gas price from `block_horizon`
    /// - `Change` or `Destroy` outputs for all assets from the inputs
    /// - `Variable` outputs in the case they are required during the execution
    /// - `Contract` inputs and outputs in the case they are required during the execution
    /// - Reserved witness slots for signed coins filled with `64` zeroes
    /// - Set script gas limit(unless `script` is empty)
    /// - Estimated predicates, if `estimate_predicates == true`
    ///
    /// Returns an error if:
    /// - The number of required balances exceeds the maximum number of inputs allowed.
    /// - The fee address index is out of bounds.
    /// - The same asset has multiple change policies(either the receiver of
    ///     the change is different, or one of the policies states about the destruction
    ///     of the token while the other does not). The `Change` output from the transaction
    ///     also count as a `ChangePolicy`.
    /// - The number of excluded coin IDs exceeds the maximum number of inputs allowed.
    /// - Required assets have multiple entries.
    /// - If accounts don't have sufficient amounts to cover the transaction requirements in assets.
    /// - If a constructed transaction breaks the rules defined by consensus parameters.
    #[graphql(complexity = "query_costs().assemble_tx")]
    #[allow(clippy::too_many_arguments)]
    async fn assemble_tx(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            desc = "The original transaction that contains application level logic only"
        )]
        tx: HexString<'_>,
        #[graphql(
            desc = "Number of blocks into the future to estimate the gas price for"
        )]
        block_horizon: U32,
        #[graphql(
            desc = "The list of required balances for the transaction to include as inputs. \
                    The list should be created based on the application-required assets. \
                    The base asset requirement should not require assets to cover the \
                    transaction fee, which will be calculated and added automatically \
                    at the end of the assembly process."
        )]
        required_balances: Vec<schema_types::RequiredBalance>,
        #[graphql(desc = "The index from the `required_balances` list \
                that points to the address who pays fee for the transaction. \
                If you only want to cover the fee of transaction, you can set the required balance \
                to 0, set base asset and point to this required address.")]
        fee_address_index: U16,
        #[graphql(
            desc = "The list of resources to exclude from the selection for the inputs"
        )]
        exclude_input: Option<ExcludeInput>,
        #[graphql(
            desc = "Perform the estimation of the predicates before cover fee of the transaction"
        )]
        estimate_predicates: Option<bool>,
        #[graphql(
            desc = "During the phase of the fee calculation, adds `reserve_gas` to the \
                    total gas used by the transaction and fetch assets to cover the fee."
        )]
        reserve_gas: Option<U64>,
    ) -> async_graphql::Result<AssembleTransactionResult> {
        let consensus_parameters = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();

        let max_input = consensus_parameters.tx_params().max_inputs();

        if required_balances.len() > max_input as usize {
            return Err(CoinsQueryError::TooManyCoinsSelected {
                required: required_balances.len(),
                max: max_input,
            }
            .into());
        }

        let fee_index: u16 = fee_address_index.into();
        let estimate_predicates: bool = estimate_predicates.unwrap_or(false);
        let reserve_gas: u64 = reserve_gas.map(Into::into).unwrap_or(0);

        let excluded_id_count = exclude_input.as_ref().map_or(0, |exclude| {
            exclude.utxos.len().saturating_add(exclude.messages.len())
        });
        if excluded_id_count > max_input as usize {
            return Err(CoinsQueryError::TooManyExcludedId {
                provided: excluded_id_count,
                allowed: max_input,
            }
            .into());
        }

        let required_balances: Vec<RequiredBalance> =
            required_balances.into_iter().map(Into::into).collect();
        let exclude: Exclude = exclude_input.into();

        let gas_price = ctx.estimate_gas_price(Some(block_horizon.into()))?;
        let config = &ctx.data_unchecked::<GraphQLConfig>().config;

        let tx = FuelTx::from_bytes(&tx.0)?;

        let read_view = Arc::new(ctx.read_view()?.into_owned());
        let block_producer = ctx.data_unchecked::<BlockProducer>();
        let shared_memory_pool = ctx.data_unchecked::<SharedMemoryPool>();

        let arguments = AssembleArguments {
            fee_index,
            required_balances,
            exclude,
            estimate_predicates,
            reserve_gas,
            consensus_parameters,
            gas_price,
            dry_run_limit: config.assemble_tx_dry_run_limit,
            estimate_predicates_limit: config.assemble_tx_estimate_predicates_limit,
            block_producer,
            read_view,
            shared_memory_pool,
        };

        let assembled_tx: fuel_tx::Transaction = match tx {
            fuel_tx::Transaction::Script(tx) => {
                AssembleTx::new(tx, arguments)?.assemble().await?.into()
            }
            fuel_tx::Transaction::Create(tx) => {
                AssembleTx::new(tx, arguments)?.assemble().await?.into()
            }
            fuel_tx::Transaction::Mint(_) => {
                return Err(anyhow::anyhow!("Mint transaction is not supported").into());
            }
            fuel_tx::Transaction::Upgrade(tx) => {
                AssembleTx::new(tx, arguments)?.assemble().await?.into()
            }
            fuel_tx::Transaction::Upload(tx) => {
                AssembleTx::new(tx, arguments)?.assemble().await?.into()
            }
            fuel_tx::Transaction::Blob(tx) => {
                AssembleTx::new(tx, arguments)?.assemble().await?.into()
            }
        };

        let (assembled_tx, status) = block_producer
            .dry_run_txs(
                vec![assembled_tx],
                None,
                None,
                Some(false),
                Some(gas_price),
                false,
            )
            .await?
            .transactions
            .into_iter()
            .next()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to do the final `dry_run` of the assembled transaction"
                )
            })?;

        let result = AssembleTransactionResult {
            tx_id: status.id,
            tx: assembled_tx,
            status: status.result,
            gas_price,
        };

        Ok(result)
    }

    /// Estimate the predicate gas for the provided transaction
    #[graphql(complexity = "query_costs().estimate_predicates + child_complexity")]
    async fn estimate_predicates(
        &self,
        ctx: &Context<'_>,
        tx: HexString<'_>,
    ) -> async_graphql::Result<Transaction> {
        let query = ctx.read_view()?.into_owned();

        let tx = FuelTx::from_bytes(&tx.0)?;

        let tx = ctx.estimate_predicates(tx, query).await?;
        let chain_id = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params()
            .chain_id();

        Ok(Transaction::from_tx(tx.id(&chain_id), tx))
    }

    #[cfg(feature = "test-helpers")]
    /// Returns all possible receipts for test purposes.
    async fn all_receipts(&self) -> Vec<receipt::Receipt> {
        receipt::all_receipts()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Execute a dry-run of multiple transactions using a fork of current state, no changes are committed.
    #[graphql(
        complexity = "query_costs().dry_run * txs.len() + child_complexity * txs.len()"
    )]
    async fn dry_run(
        &self,
        ctx: &Context<'_>,
        txs: Vec<HexString<'_>>,
        // If set to false, disable input utxo validation, overriding the configuration of the node.
        // This allows for non-existent inputs to be used without signature validation
        // for read-only calls.
        utxo_validation: Option<bool>,
        gas_price: Option<U64>,
        // This can be used to run the dry-run on top of a past block.
        // Requires `--historical-execution` flag to be enabled.
        block_height: Option<U32>,
    ) -> async_graphql::Result<Vec<DryRunTransactionExecutionStatus>> {
        Ok(self
            .dry_run_inner(ctx, txs, utxo_validation, gas_price, block_height, false)
            .await?
            .tx_statuses)
    }

    /// Execute a dry-run of multiple transactions using a fork of current state, no changes are committed.
    /// Also records accesses, so the execution can be replicated locally.
    #[graphql(
        complexity = "query_costs().dry_run * txs.len() + child_complexity * txs.len()"
    )]
    async fn dry_run_record_storage_reads(
        &self,
        ctx: &Context<'_>,
        txs: Vec<HexString<'_>>,
        // If set to false, disable input utxo validation, overriding the configuration of the node.
        // This allows for non-existent inputs to be used without signature validation
        // for read-only calls.
        utxo_validation: Option<bool>,
        gas_price: Option<U64>,
        // This can be used to run the dry-run on top of a past block.
        // Requires `--historical-execution` flag to be enabled.
        block_height: Option<U32>,
    ) -> async_graphql::Result<DryRunStorageReads> {
        self.dry_run_inner(ctx, txs, utxo_validation, gas_price, block_height, true)
            .await
    }

    /// Get execution trace for an already-executed block.
    #[graphql(complexity = "query_costs().storage_read_replay + child_complexity")]
    async fn storage_read_replay(
        &self,
        ctx: &Context<'_>,
        height: U32,
    ) -> async_graphql::Result<Vec<StorageReadReplayEvent>> {
        let config = ctx.data_unchecked::<GraphQLConfig>();
        if !config.historical_execution {
            return Err(anyhow::anyhow!(
                "`--historical-execution` is required for this operation"
            )
            .into());
        }

        let block_height = height.into();
        let block_producer = ctx.data_unchecked::<BlockProducer>();
        Ok(block_producer
            .storage_read_replay(block_height)
            .await?
            .into_iter()
            .map(StorageReadReplayEvent::from)
            .collect())
    }
}

#[derive(Default)]
pub struct TxMutation;

#[Object]
impl TxMutation {
    /// Execute a dry-run of multiple transactions using a fork of current state, no changes are committed.
    #[graphql(
        complexity = "query_costs().dry_run * txs.len() + child_complexity * txs.len()",
        deprecation = "This doesn't need to be a mutation. Use query of the same name instead."
    )]
    async fn dry_run(
        &self,
        ctx: &Context<'_>,
        txs: Vec<HexString<'_>>,
        // If set to false, disable input utxo validation, overriding the configuration of the node.
        // This allows for non-existent inputs to be used without signature validation
        // for read-only calls.
        utxo_validation: Option<bool>,
        gas_price: Option<U64>,
        // This can be used to run the dry-run on top of a past block.
        // Requires `--historical-execution` flag to be enabled.
        block_height: Option<U32>,
    ) -> async_graphql::Result<Vec<DryRunTransactionExecutionStatus>> {
        TxQuery::dry_run(&TxQuery, ctx, txs, utxo_validation, gas_price, block_height)
            .await
    }

    /// Submits transaction to the `TxPool`.
    ///
    /// Returns submitted transaction if the transaction is included in the `TxPool` without problems.
    #[graphql(complexity = "query_costs().submit + child_complexity")]
    async fn submit(
        &self,
        ctx: &Context<'_>,
        tx: HexString<'_>,
        estimate_predicates: Option<bool>,
    ) -> async_graphql::Result<Transaction> {
        let txpool = ctx.data_unchecked::<TxPool>();
        let mut tx = FuelTx::from_bytes(&tx.0)?;

        if estimate_predicates.unwrap_or(false) {
            let query = ctx.read_view()?.into_owned();
            tx = ctx.estimate_predicates(tx, query).await?;
        }

        txpool
            .insert(tx.clone())
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        let chain_id = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params()
            .chain_id();
        let id = tx.id(&chain_id);

        let tx = Transaction(tx, id);
        Ok(tx)
    }
}

#[derive(Default)]
pub struct TxStatusSubscription;

#[Subscription]
impl<'a> TxStatusSubscription {
    /// Returns a stream of status updates for the given transaction id.
    /// If the current status is [`TransactionStatus::Success`], [`TransactionStatus::Failed`],
    /// or [`TransactionStatus::SqueezedOut`] the stream will return that and end immediately.
    /// Other, intermediate statuses will also be returned but the stream will remain active
    /// and wait for a future updates.
    ///
    /// This stream will wait forever so it's advised to use within a timeout.
    ///
    /// It is possible for the stream to miss an update if it is polled slower
    /// then the updates arrive. In such a case the stream will close without
    /// a status. If this occurs the stream can simply be restarted to return
    /// the latest status.
    #[graphql(complexity = "query_costs().status_change + child_complexity")]
    async fn status_change(
        &self,
        ctx: &'a Context<'a>,
        #[graphql(desc = "The ID of the transaction")] id: TransactionId,
        #[graphql(desc = "If true, accept to receive the preconfirmation status")]
        include_preconfirmation: Option<bool>,
    ) -> anyhow::Result<
        impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a + use<'a>,
    > {
        let tx_status_manager = ctx.data_unchecked::<DynTxStatusManager>();
        let rx = tx_status_manager.tx_update_subscribe(id.into()).await?;
        let query = ctx.read_view()?;

        let status_change_state = StatusChangeState {
            tx_status_manager,
            query,
        };
        Ok(transaction_status_change(
            status_change_state,
            rx,
            id.into(),
            include_preconfirmation.unwrap_or(false),
        )
        .await
        .map_err(async_graphql::Error::from))
    }

    /// Submits transaction to the `TxPool` and await either success or failure.
    #[graphql(complexity = "query_costs().submit_and_await + child_complexity")]
    async fn submit_and_await(
        &self,
        ctx: &'a Context<'a>,
        tx: HexString<'a>,
        estimate_predicates: Option<bool>,
    ) -> async_graphql::Result<
        impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a + use<'a>,
    > {
        use tokio_stream::StreamExt;
        let subscription =
            submit_and_await_status(ctx, tx, estimate_predicates.unwrap_or(false), false)
                .await?;

        Ok(subscription
            .skip_while(|event| event.as_ref().map_or(true, |status| !status.is_final()))
            .take(1))
    }

    /// Submits the transaction to the `TxPool` and returns a stream of events.
    /// Compared to the `submitAndAwait`, the stream also contains
    /// `SubmittedStatus` and potentially preconfirmation as an intermediate state.
    #[graphql(complexity = "query_costs().submit_and_await + child_complexity")]
    async fn submit_and_await_status(
        &'a self,
        ctx: &'a Context<'a>,
        tx: HexString<'a>,
        estimate_predicates: Option<bool>,
        include_preconfirmation: Option<bool>,
    ) -> async_graphql::Result<
        impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a + use<'a>,
    > {
        submit_and_await_status(
            ctx,
            tx,
            estimate_predicates.unwrap_or(false),
            include_preconfirmation.unwrap_or(false),
        )
        .await
    }
}

async fn submit_and_await_status<'a>(
    ctx: &'a Context<'a>,
    tx: HexString<'a>,
    estimate_predicates: bool,
    include_preconfirmation: bool,
) -> async_graphql::Result<
    impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a,
> {
    use tokio_stream::StreamExt;
    let txpool = ctx.data_unchecked::<TxPool>();
    let tx_status_manager = ctx.data_unchecked::<DynTxStatusManager>();
    let params = ctx
        .data_unchecked::<ChainInfoProvider>()
        .current_consensus_params();
    let mut tx = FuelTx::from_bytes(&tx.0)?;
    let tx_id = tx.id(&params.chain_id());

    if estimate_predicates {
        let query = ctx.read_view()?.into_owned();
        tx = ctx.estimate_predicates(tx, query).await?;
    }

    let subscription = tx_status_manager.tx_update_subscribe(tx_id).await?;

    txpool.insert(tx).await?;

    Ok(subscription
        .filter_map(move |status| {
            match status {
                TxStatusMessage::Status(status) => {
                    let status = TransactionStatus::new(tx_id, status);
                    if !include_preconfirmation && status.is_preconfirmation() {
                        None
                    } else {
                        Some(Ok(status))
                    }
                }
                // Map a failed status to an error for the api.
                TxStatusMessage::FailedStatus => Some(Err(anyhow::anyhow!(
                    "Failed to get transaction status"
                )
                .into())),
            }
        })
        .take(3))
}

struct StatusChangeState<'a> {
    query: Cow<'a, ReadView>,
    tx_status_manager: &'a DynTxStatusManager,
}

impl TxnStatusChangeState for StatusChangeState<'_> {
    async fn get_tx_status(
        &self,
        id: Bytes32,
        include_preconfirmation: bool,
    ) -> StorageResult<Option<transaction_status::TransactionStatus>> {
        get_tx_status(
            &id,
            self.query.as_ref(),
            self.tx_status_manager,
            include_preconfirmation,
        )
        .await
    }
}

pub mod schema_types {
    use super::*;

    #[derive(async_graphql::Enum, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Destroy {
        Destroy,
    }

    #[derive(async_graphql::OneofObject)]
    pub enum ChangePolicy {
        /// Adds `Output::Change` to the transaction if it is not already present.
        /// Sending remaining assets to the provided address.
        Change(Address),
        /// Destroys the remaining assets by the transaction for provided address.
        Destroy(Destroy),
    }

    #[derive(async_graphql::OneofObject)]
    pub enum Account {
        Address(Address),
        Predicate(Predicate),
    }

    #[derive(async_graphql::InputObject)]
    pub struct Predicate {
        // The address of the predicate can be different from the actual bytecode.
        // This feature is used by wallets during estimation of the predicate that requires
        // signature verification. They provide a mocked version of the predicate that
        // returns `true` even if the signature doesn't match.
        pub predicate_address: Address,
        pub predicate: HexString<'static>,
        pub predicate_data: HexString<'static>,
    }

    #[derive(async_graphql::InputObject)]
    pub struct RequiredBalance {
        pub asset_id: AssetId,
        pub amount: U64,
        pub account: Account,
        pub change_policy: ChangePolicy,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChangePolicy {
    /// Adds `Output::Change` to the transaction if it is not already present.
    /// Sending remaining assets to the provided address.
    Change(fuel_tx::Address),
    /// Destroys the remaining assets by the transaction for provided address.
    Destroy,
}

#[derive(Clone)]
pub enum Account {
    Address(fuel_tx::Address),
    Predicate(Predicate),
}

impl Account {
    pub fn owner(&self) -> fuel_tx::Address {
        match self {
            Account::Address(address) => *address,
            Account::Predicate(predicate) => predicate.predicate_address,
        }
    }
}

#[derive(Clone)]
pub struct Predicate {
    pub predicate_address: fuel_tx::Address,
    pub predicate: Vec<u8>,
    pub predicate_data: Vec<u8>,
}

struct RequiredBalance {
    asset_id: fuel_tx::AssetId,
    amount: fuel_tx::Word,
    account: Account,
    change_policy: ChangePolicy,
}

impl From<schema_types::RequiredBalance> for RequiredBalance {
    fn from(required_balance: schema_types::RequiredBalance) -> Self {
        let asset_id: fuel_tx::AssetId = required_balance.asset_id.into();
        let amount: fuel_tx::Word = required_balance.amount.into();
        let account = match required_balance.account {
            schema_types::Account::Address(address) => Account::Address(address.into()),
            schema_types::Account::Predicate(predicate) => {
                let predicate_address = predicate.predicate_address.into();
                let predicate_data = predicate.predicate_data.into();
                let predicate = predicate.predicate.into();
                Account::Predicate(Predicate {
                    predicate_address,
                    predicate,
                    predicate_data,
                })
            }
        };

        let change_policy = match required_balance.change_policy {
            schema_types::ChangePolicy::Change(address) => {
                ChangePolicy::Change(address.into())
            }
            schema_types::ChangePolicy::Destroy(_) => ChangePolicy::Destroy,
        };

        Self {
            asset_id,
            amount,
            account,
            change_policy,
        }
    }
}

pub trait ContextExt {
    fn try_find_tx(
        &self,
        id: Bytes32,
    ) -> impl Future<Output = StorageResult<Option<FuelTx>>> + Send;

    fn estimate_predicates(
        &self,
        tx: FuelTx,
        query: impl PredicateStorageRequirements + Send + Sync + 'static,
    ) -> impl Future<Output = anyhow::Result<FuelTx>> + Send;
}

impl ContextExt for Context<'_> {
    async fn try_find_tx(&self, id: Bytes32) -> StorageResult<Option<FuelTx>> {
        let query = self.read_view()?;
        let txpool = self.data_unchecked::<TxPool>();

        match txpool.transaction(id).await? {
            Some(tx) => Ok(Some(tx)),
            _ => {
                let result = query.transaction(&id);

                if result.is_not_found() {
                    Ok(None)
                } else {
                    result.map(Some)
                }
            }
        }
    }

    async fn estimate_predicates(
        &self,
        mut tx: FuelTx,
        query: impl PredicateStorageRequirements + Send + Sync + 'static,
    ) -> anyhow::Result<FuelTx> {
        let mut has_predicates = false;

        for input in tx.inputs().iter() {
            if input.predicate().is_some() {
                has_predicates = true;
                break;
            }
        }

        if !has_predicates {
            return Ok(tx);
        }

        let params = self
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();

        let memory_pool = self.data_unchecked::<SharedMemoryPool>();
        let memory = memory_pool.get_memory().await;

        let parameters = CheckPredicateParams::from(params.as_ref());
        let tx = tokio_rayon::spawn_fifo(move || {
            let result = tx.estimate_predicates(&parameters, memory, &query);
            result.map(|_| tx)
        })
        .await
        .map_err(|err| anyhow::anyhow!("{:?}", err))?;

        Ok(tx)
    }
}
