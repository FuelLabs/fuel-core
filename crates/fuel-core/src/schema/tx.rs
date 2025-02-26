use super::scalars::{
    AssetId,
    U32,
    U64,
    U8,
};
use crate::{
    coins_query::CoinsQueryError,
    fuel_core_graphql_api::{
        api_service::{
            BlockProducer,
            ChainInfoProvider,
            TxPool,
        },
        query_costs,
        Config as GraphQLConfig,
        IntoApiResult,
    },
    graphql_api::{
        database::ReadView,
        ports::MemoryPool,
    },
    query::{
        asset_query::Exclude,
        transaction_status_change,
        TxnStatusChangeState,
    },
    schema::{
        coins::{
            CoinType,
            ExcludeInput,
            SpendQueryElementInput,
        },
        gas_price::EstimateGasPriceExt,
        scalars::{
            Address,
            HexString,
            SortedTxCursor,
            TransactionId,
            TxPointer,
        },
        tx::types::{
            AssembleTransactionResult,
            TransactionStatus,
        },
        ReadViewProvider,
    },
    service::adapters::SharedMemoryPool,
};
use async_graphql::{
    connection::{
        Connection,
        EmptyFields,
    },
    Context,
    Object,
    Subscription,
};
use fuel_core_executor::ports::{
    TransactionExt,
    TransactionWriteExt,
};
use fuel_core_storage::{
    iter::IterDirection,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_txpool::TxStatusMessage;
use fuel_core_types::{
    entities::coins::CoinId,
    fuel_tx,
    fuel_tx::{
        input::{
            coin::CoinSigned,
            message::{
                MessageCoinSigned,
                MessageDataSigned,
            },
        },
        Bytes32,
        Cacheable,
        Transaction as FuelTx,
        UniqueIdentifier,
    },
    fuel_types::{
        self,
        canonical::Deserialize,
    },
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
        },
        Signature,
    },
    services::txpool,
};
use futures::{
    Stream,
    TryStreamExt,
};
use std::{
    borrow::Cow,
    collections::{
        hash_map::Entry,
        HashMap,
        HashSet,
    },
    iter,
};
use types::{
    DryRunTransactionExecutionStatus,
    StorageReadReplayEvent,
    Transaction,
};

pub mod input;
pub mod output;
pub mod receipt;
pub mod types;
pub mod upgrade_purpose;

#[derive(Default)]
pub struct TxQuery;

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

        if let Some(transaction) = txpool.transaction(id).await? {
            Ok(Some(Transaction(transaction, id)))
        } else {
            query
                .transaction(&id)
                .map(|tx| Transaction::from_tx(id, tx))
                .into_api_result()
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
    async fn assemble_tx(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            desc = "The original transaction that contains application level logic only"
        )]
        tx: HexString,
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
        fee_address_index: U8,
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
        let consensus_params = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();

        let max_input = consensus_params.tx_params().max_inputs();

        if required_balances.len() > max_input as usize {
            return Err(CoinsQueryError::TooManyCoinsSelected {
                required: required_balances.len(),
                max: max_input,
            }
            .into());
        }

        let fee_index: u8 = fee_address_index.into();

        if fee_index as usize >= required_balances.len() {
            return Err(anyhow::anyhow!("The fee address index is out of bounds").into());
        }

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
        let mut duplicate_checker = HashSet::with_capacity(required_balances.len());
        for required_balance in &required_balances {
            let asset_id = required_balance.asset_id;
            if !duplicate_checker.insert(asset_id) {
                return Err(CoinsQueryError::DuplicateAssets(asset_id).into());
            }
        }

        let gas_price = ctx.estimate_gas_price(Some(block_horizon.into()))?;

        let mut tx = FuelTx::from_bytes(&tx.0)?;

        let mut change_output_policies = HashMap::<fuel_tx::AssetId, ChangePolicy>::new();
        let mut available_change_outputs = HashSet::<fuel_tx::AssetId>::new();

        for output in tx.outputs()? {
            if let fuel_tx::Output::Change { to, asset_id, .. } = output {
                change_output_policies
                    .insert(*asset_id, ChangePolicy::Change((*to).into()));
                available_change_outputs.insert(*asset_id);
            }
        }

        for required_balance in &required_balances {
            let asset_id = required_balance.asset_id;

            let entry = change_output_policies.entry(asset_id);

            match entry {
                Entry::Occupied(old) => {
                    if old.get() != &required_balance.change_policy {
                        return Err(anyhow::anyhow!(
                            "The asset {} has multiple change policies",
                            asset_id
                        )
                        .into());
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(required_balance.change_policy);
                }
            }
        }

        let mut exclude: Exclude = exclude_input.into();
        let mut signature_witness_indexes = HashMap::<fuel_tx::Address, u16>::new();

        // Exclude inputs that already are used by the transaction
        let inputs = tx.inputs()?;
        for input in inputs {
            if let Some(utxo_id) = input.utxo_id() {
                exclude.exclude(CoinId::Utxo(*utxo_id));
            }

            if let Some(nonce) = input.nonce() {
                exclude.exclude(CoinId::Message(*nonce));
            }

            match input {
                fuel_tx::Input::CoinSigned(CoinSigned {
                    owner,
                    witness_index,
                    ..
                })
                | fuel_tx::Input::MessageCoinSigned(MessageCoinSigned {
                    recipient: owner,
                    witness_index,
                    ..
                })
                | fuel_tx::Input::MessageDataSigned(MessageDataSigned {
                    recipient: owner,
                    witness_index,
                    ..
                }) => {
                    signature_witness_indexes.insert(*owner, *witness_index);
                }
                fuel_tx::Input::Contract(_)
                | fuel_tx::Input::CoinPredicate(_)
                | fuel_tx::Input::MessageCoinPredicate(_)
                | fuel_tx::Input::MessageDataPredicate(_) => {
                    // Do nothing
                }
            }
        }

        let read_view = ctx.read_view()?;
        let mut number_of_witnesses =
            u16::try_from(tx.witnesses()?.len()).unwrap_or(u16::MAX);

        for required_balance in &required_balances {
            let used_inputs = u16::try_from(tx.inputs()?.len()).unwrap_or(u16::MAX);
            let remaining_input_slots = max_input.saturating_sub(used_inputs);
            if remaining_input_slots == 0 {
                return Err(anyhow::anyhow!(
                    "Filling required balances occupies a number \
                    of inputs more than can fit into the transaction"
                )
                .into());
            }

            let asset_id = required_balance.asset_id;
            let query_per_asset = SpendQueryElementInput {
                asset_id: asset_id.into(),
                amount: (required_balance.amount as u128).into(),
                max: None,
            };
            let owner = required_balance.account.owner();

            let selected_coins = read_view
                .coins_to_spend(
                    owner,
                    &[query_per_asset],
                    &exclude,
                    &consensus_params,
                    remaining_input_slots,
                )
                .await?
                .into_iter()
                .next()
                .expect("The query returns a single result; qed");

            if available_change_outputs.insert(asset_id) {
                match change_output_policies
                    .get(&asset_id)
                    .expect("Policy was inserted above; qed")
                {
                    ChangePolicy::Change(change_receiver) => {
                        tx.outputs_mut()?.push(fuel_tx::Output::change(
                            *change_receiver,
                            0,
                            asset_id,
                        ));
                    }
                    ChangePolicy::Destroy => {
                        // Do nothing for now, since `fuel-tx` crate doesn't have
                        // `Destroy` output yet.
                        // https://github.com/FuelLabs/fuel-specs/issues/621
                    }
                }
            }

            for coin in selected_coins {
                let mut add_witness = None;

                let input = match &required_balance.account {
                    Account::Address(account) => {
                        let signature_index = signature_witness_indexes
                            .get(account)
                            .cloned()
                            .unwrap_or_else(|| {
                                let vacant_index = number_of_witnesses;
                                add_witness = Some((*account, vacant_index));

                                vacant_index
                            });

                        match coin {
                            CoinType::Coin(coin) => fuel_tx::Input::coin_signed(
                                coin.0.utxo_id,
                                coin.0.owner,
                                coin.0.amount,
                                coin.0.asset_id,
                                coin.0.tx_pointer,
                                signature_index,
                            ),
                            CoinType::MessageCoin(message) => {
                                fuel_tx::Input::message_coin_signed(
                                    message.0.sender,
                                    message.0.recipient,
                                    message.0.amount,
                                    message.0.nonce,
                                    signature_index,
                                )
                            }
                        }
                    }
                    Account::Predicate(predicate) => {
                        let predicate_gas_used = 0;
                        match coin {
                            CoinType::Coin(coin) => fuel_tx::Input::coin_predicate(
                                coin.0.utxo_id,
                                predicate.predicate_address,
                                coin.0.amount,
                                coin.0.asset_id,
                                coin.0.tx_pointer,
                                predicate_gas_used,
                                predicate.predicate.clone(),
                                predicate.predicate_data.clone(),
                            ),
                            CoinType::MessageCoin(message) => {
                                fuel_tx::Input::message_coin_predicate(
                                    message.0.sender,
                                    message.0.recipient,
                                    message.0.amount,
                                    message.0.nonce,
                                    predicate_gas_used,
                                    predicate.predicate.clone(),
                                    predicate.predicate_data.clone(),
                                )
                            }
                        }
                    }
                };
                tx.inputs_mut()?.push(input);

                if let Some((owner, index)) = add_witness {
                    tx.witnesses_mut()?.push(vec![0; Signature::LEN].into());
                    signature_witness_indexes.insert(owner, index);
                    number_of_witnesses = number_of_witnesses.saturating_add(1);
                }
            }
        }

        let block_producer = ctx.data_unchecked::<BlockProducer>();
        todo!()

        // if block_height.is_some() && !config.historical_execution {
        //     return Err(anyhow::anyhow!(
        //         "The `blockHeight` parameter requires the `--historical-execution` option"
        //     )
        //     .into());
        // }
        //
        // let mut transactions = txs
        //     .iter()
        //     .map(|tx| FuelTx::from_bytes(&tx.0))
        //     .collect::<Result<Vec<FuelTx>, _>>()?;
        // transactions.iter_mut().try_fold::<_, _, async_graphql::Result<u64>>(0u64, |acc, tx| {
        //     let gas = tx.max_gas(&consensus_params)?;
        //     let gas = gas.saturating_add(acc);
        //     if gas > block_gas_limit {
        //         return Err(anyhow::anyhow!("The sum of the gas usable by the transactions is greater than the block gas limit").into());
        //     }
        //     tx.precompute(&consensus_params.chain_id())?;
        //     Ok(gas)
        // })?;
        //
        // let tx_statuses = block_producer
        //     .dry_run_txs(
        //         transactions,
        //         block_height.map(|x| x.into()),
        //         None, // TODO(#1749): Pass parameter from API
        //         utxo_validation,
        //         gas_price.map(|x| x.into()),
        //     )
        //     .await?;
        // let tx_statuses = tx_statuses
        //     .into_iter()
        //     .map(DryRunTransactionExecutionStatus)
        //     .collect();
        //
        // Ok(tx_statuses)
    }

    /// Estimate the predicate gas for the provided transaction
    #[graphql(complexity = "query_costs().estimate_predicates + child_complexity")]
    async fn estimate_predicates(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<Transaction> {
        let query = ctx.read_view()?.into_owned();

        let mut tx = FuelTx::from_bytes(&tx.0)?;

        let params = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();

        let memory_pool = ctx.data_unchecked::<SharedMemoryPool>();
        let memory = memory_pool.get_memory().await;

        let parameters = CheckPredicateParams::from(params.as_ref());
        let tx = tokio_rayon::spawn_fifo(move || {
            let result = tx.estimate_predicates(&parameters, memory, &query);
            result.map(|_| tx)
        })
        .await
        .map_err(|err| anyhow::anyhow!("{:?}", err))?;

        Ok(Transaction::from_tx(tx.id(&params.chain_id()), tx))
    }

    #[cfg(feature = "test-helpers")]
    /// Returns all possible receipts for test purposes.
    async fn all_receipts(&self) -> Vec<receipt::Receipt> {
        receipt::all_receipts()
            .into_iter()
            .map(Into::into)
            .collect()
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
        complexity = "query_costs().dry_run * txs.len() + child_complexity * txs.len()"
    )]
    async fn dry_run(
        &self,
        ctx: &Context<'_>,
        txs: Vec<HexString>,
        // If set to false, disable input utxo validation, overriding the configuration of the node.
        // This allows for non-existent inputs to be used without signature validation
        // for read-only calls.
        utxo_validation: Option<bool>,
        gas_price: Option<U64>,
        // This can be used to run the dry-run on top of a past block.
        // Requires `--historical-execution` flag to be enabled.
        block_height: Option<U32>,
    ) -> async_graphql::Result<Vec<DryRunTransactionExecutionStatus>> {
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

        let tx_statuses = block_producer
            .dry_run_txs(
                transactions,
                block_height.map(|x| x.into()),
                None, // TODO(#1749): Pass parameter from API
                utxo_validation,
                gas_price.map(|x| x.into()),
            )
            .await?;
        let tx_statuses = tx_statuses
            .into_iter()
            .map(DryRunTransactionExecutionStatus)
            .collect();

        Ok(tx_statuses)
    }

    /// Submits transaction to the `TxPool`.
    ///
    /// Returns submitted transaction if the transaction is included in the `TxPool` without problems.
    #[graphql(complexity = "query_costs().submit + child_complexity")]
    async fn submit(
        &self,
        ctx: &Context<'_>,
        tx: HexString,
    ) -> async_graphql::Result<Transaction> {
        let txpool = ctx.data_unchecked::<TxPool>();
        let params = ctx
            .data_unchecked::<ChainInfoProvider>()
            .current_consensus_params();
        let tx = FuelTx::from_bytes(&tx.0)?;

        txpool
            .insert(tx.clone())
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        let id = tx.id(&params.chain_id());

        let tx = Transaction(tx, id);
        Ok(tx)
    }
}

#[derive(Default)]
pub struct TxStatusSubscription;

#[Subscription]
impl TxStatusSubscription {
    /// Returns a stream of status updates for the given transaction id.
    /// If the current status is [`TransactionStatus::Success`], [`TransactionStatus::SqueezedOut`]
    /// or [`TransactionStatus::Failed`] the stream will return that and end immediately.
    /// If the current status is [`TransactionStatus::Submitted`] this will be returned
    /// and the stream will wait for a future update.
    ///
    /// This stream will wait forever so it's advised to use within a timeout.
    ///
    /// It is possible for the stream to miss an update if it is polled slower
    /// then the updates arrive. In such a case the stream will close without
    /// a status. If this occurs the stream can simply be restarted to return
    /// the latest status.
    #[graphql(complexity = "query_costs().status_change + child_complexity")]
    async fn status_change<'a>(
        &self,
        ctx: &'a Context<'a>,
        #[graphql(desc = "The ID of the transaction")] id: TransactionId,
    ) -> anyhow::Result<impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a>
    {
        let txpool = ctx.data_unchecked::<TxPool>();
        let rx = txpool.tx_update_subscribe(id.into())?;
        let query = ctx.read_view()?;

        let status_change_state = StatusChangeState { txpool, query };
        Ok(
            transaction_status_change(status_change_state, rx, id.into())
                .await
                .map_err(async_graphql::Error::from),
        )
    }

    /// Submits transaction to the `TxPool` and await either confirmation or failure.
    #[graphql(complexity = "query_costs().submit_and_await + child_complexity")]
    async fn submit_and_await<'a>(
        &self,
        ctx: &'a Context<'a>,
        tx: HexString,
    ) -> async_graphql::Result<
        impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a,
    > {
        use tokio_stream::StreamExt;
        let subscription = submit_and_await_status(ctx, tx).await?;

        Ok(subscription
            .skip_while(|event| matches!(event, Ok(TransactionStatus::Submitted(..))))
            .take(1))
    }

    /// Submits the transaction to the `TxPool` and returns a stream of events.
    /// Compared to the `submitAndAwait`, the stream also contains `
    /// SubmittedStatus` as an intermediate state.
    #[graphql(complexity = "query_costs().submit_and_await + child_complexity")]
    async fn submit_and_await_status<'a>(
        &self,
        ctx: &'a Context<'a>,
        tx: HexString,
    ) -> async_graphql::Result<
        impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a,
    > {
        submit_and_await_status(ctx, tx).await
    }
}

async fn submit_and_await_status<'a>(
    ctx: &'a Context<'a>,
    tx: HexString,
) -> async_graphql::Result<
    impl Stream<Item = async_graphql::Result<TransactionStatus>> + 'a,
> {
    use tokio_stream::StreamExt;
    let txpool = ctx.data_unchecked::<TxPool>();
    let params = ctx
        .data_unchecked::<ChainInfoProvider>()
        .current_consensus_params();
    let tx = FuelTx::from_bytes(&tx.0)?;
    let tx_id = tx.id(&params.chain_id());
    let subscription = txpool.tx_update_subscribe(tx_id)?;

    txpool.insert(tx).await?;

    Ok(subscription
        .map(move |event| match event {
            TxStatusMessage::Status(status) => {
                let status = TransactionStatus::new(tx_id, status);
                Ok(status)
            }
            TxStatusMessage::FailedStatus => {
                Err(anyhow::anyhow!("Failed to get transaction status").into())
            }
        })
        .take(2))
}

struct StatusChangeState<'a> {
    query: Cow<'a, ReadView>,
    txpool: &'a TxPool,
}

impl<'a> TxnStatusChangeState for StatusChangeState<'a> {
    async fn get_tx_status(
        &self,
        id: Bytes32,
    ) -> StorageResult<Option<txpool::TransactionStatus>> {
        match self.query.tx_status(&id) {
            Ok(status) => Ok(Some(status)),
            Err(StorageError::NotFound(_, _)) => Ok(self
                .txpool
                .submission_time(id)
                .await
                .map_err(|e| anyhow::anyhow!(e))?
                .map(|time| txpool::TransactionStatus::Submitted { time })),
            Err(err) => Err(err),
        }
    }
}

pub mod schema_types {
    use super::*;

    #[derive(async_graphql::OneofObject)]
    pub enum ChangePolicy {
        /// Adds `Output::Change` to the transaction if it is not already present.
        /// Sending remaining assets to the provided address.
        Change(Address),
        /// Destroys the remaining assets by the transaction for provided address.
        Destroy(bool),
    }

    #[derive(async_graphql::OneofObject)]
    pub enum Account {
        Address(Address),
        Predicate(Predicate),
    }

    #[derive(async_graphql::InputObject)]
    pub struct Predicate {
        // The address of the predicate can be different from teh actual bytecode.
        // This feature is used by wallets during estimation of the predicate that requires
        // signature verification. They provide a mocked version of the predicate that
        // returns `true` even if the signature doesn't match.
        pub predicate_address: Address,
        pub predicate: Vec<u8>,
        pub predicate_data: Vec<u8>,
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
                let predicate_data = predicate.predicate_data;
                let predicate = predicate.predicate;
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
