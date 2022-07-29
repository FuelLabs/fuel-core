use crate::{config, finalization_queue::FinalizationQueue, service::Context};
use anyhow::Error;
use ethers_core::types::{BlockId, Filter, Log, TxHash, ValueOrArray, H256};
use ethers_providers::{FilterWatcher, Middleware, ProviderError, StreamExt, SyncingStatus};
use fuel_core_interfaces::{
    block_importer::ImportBlockBroadcast,
    model::DaBlockHeight,
    relayer::{RelayerError, RelayerRequest, RelayerStatus},
};
use std::{
    cmp::{max, min},
    sync::Arc,
    time::Duration,
};
use tracing::{debug, error, info, trace};

pub struct Relayer {
    /// Pending stakes/assets/withdrawals. Before they are finalized
    queue: FinalizationQueue,
    /// state of relayer
    status: RelayerStatus,
    /// context,
    ctx: Context,
}

macro_rules! handle_interrupt {
    ($relayer:ident, $x:expr) => {
        loop {
            tokio::select! {
                biased;
                inner_fuel_event = $relayer.ctx.receiver.recv() => {
                    tracing::info!("Received event in stop handle:{:?}", inner_fuel_event);
                    match inner_fuel_event {
                        Some(RelayerRequest::Stop) | None =>{
                            $relayer.status = RelayerStatus::Stop;
                            break Err(RelayerError::Stopped);
                        },
                        Some(RelayerRequest::GetValidatorSet {response, .. }) => {
                            let _ = response.send(Err(RelayerError::ValidatorSetEthClientSyncing));
                        },
                        Some(RelayerRequest::GetStatus { response }) => {
                            let _ = response.send($relayer.status);
                        },
                    }
                }
                o = $x => {
                    break Ok(o)
                }
            }
        }
    };
}

impl Relayer {
    pub async fn new(ctx: Context) -> Self {
        let chain_height = ctx.db.get_chain_height().await;
        let last_committed_finalized_fuel_height =
            ctx.db.get_last_committed_finalized_fuel_height().await;

        let queue = FinalizationQueue::new(
            ctx.config.eth_chain_id(),
            ctx.config.eth_v2_commit_contract(),
            &ctx.private_key,
            chain_height,
            last_committed_finalized_fuel_height,
        );

        Self {
            queue,
            status: RelayerStatus::DaClientIsSyncing,
            ctx,
        }
    }

    /// Initial syncing from ethereum logs into fuel database. It does overlapping synchronization and returns
    /// logs watcher with assurance that we didn't miss any events.
    //#[tracing::instrument(skip_all)]
    async fn initial_sync<'a, P>(
        &mut self,
        provider: &'a P,
    ) -> Result<
        (
            DaBlockHeight,
            FilterWatcher<'a, P::Provider, TxHash>,
            FilterWatcher<'a, P::Provider, Log>,
        ),
        Error,
    >
    where
        P: Middleware<Error = ProviderError>,
    {
        info!("initial sync");
        // loop and wait for eth client to finish syncing
        loop {
            if matches!(
                handle_interrupt!(self, provider.syncing())??,
                SyncingStatus::IsFalse
            ) {
                break;
            }
            let wait = self.ctx.config.initial_sync_refresh();
            handle_interrupt!(self, tokio::time::sleep(wait))?;
        }

        info!("da client is synced");

        let last_finalized_da_height = std::cmp::max(
            self.ctx.config.eth_v2_contracts_deployment(),
            self.ctx.db.get_finalized_da_height().await,
        );
        // should be always more then last finalized_da_heights

        let best_finalized_block =
            provider.get_block_number().await?.as_u64() - self.ctx.config.da_finalization();

        // 1. sync from HardCoddedContractCreatingBlock->BestEthBlock-100)
        let step = self.ctx.config.initial_sync_step(); // do some stats on optimal value
        let contracts = self.ctx.config.eth_v2_listening_contracts().to_vec();
        // on start of contract there is possibility of them being overlapping, so we want to skip for loop
        // with next line
        let best_finalized_block = max(last_finalized_da_height, best_finalized_block);
        info!(
            "get logs from:{} to best finalized block:{}",
            last_finalized_da_height, best_finalized_block
        );

        for start in (last_finalized_da_height..best_finalized_block).step_by(step) {
            let end = min(start + step as DaBlockHeight, best_finalized_block);
            if (start - last_finalized_da_height) % config::REPORT_INIT_SYNC_PROGRESS_EVERY_N_BLOCKS
                == 0
            {
                info!("getting log from height:{}", start);
            }

            // TODO  can be parallelized
            let filter = Filter::new()
                .from_block(start)
                .to_block(end)
                .address(ValueOrArray::Array(contracts.clone()));
            let logs = handle_interrupt!(self, provider.get_logs(&filter))??;
            self.queue.append_eth_logs(logs).await;

            // we are sending dummy eth block bcs we are sure that it is finalized
            self.queue.commit_diffs(self.ctx.db.as_mut(), end).await;
        }

        // TODO probably not needed now. but after some time we will need to do sync to best block here.
        // it depends on how much time it is needed to traverse first part of this function
        // and how much lag happened in meantime.

        let mut watchers: Option<(FilterWatcher<_, _>, FilterWatcher<_, _>)>;
        let last_included_block = best_finalized_block;

        let mut best_block;

        loop {
            // 1. get best block and its hash sync over it, and push it over
            self.queue.clear();

            best_block = handle_interrupt!(self, provider.get_block_number())??;
            // there is not get block latest from ethers so we need to do it in two steps to get hash

            let block = handle_interrupt!(self, provider.get_block(best_block))??
                .ok_or(RelayerError::InitialSyncAskedForUnknownBlock)?;
            let best_block_hash = block.hash.unwrap(); // it is okay to unwrap

            // 2. sync overlap from LastIncludedEthBlock-> BestEthBlock) they are saved in dequeue.
            let filter = Filter::new()
                .from_block(last_included_block)
                .to_block(best_block)
                .address(ValueOrArray::Array(contracts.clone()));

            let logs = handle_interrupt!(self, provider.get_logs(&filter))??;
            self.queue.append_eth_logs(logs).await;

            // 3. Start listening to eth events
            let eth_log_filter = Filter::new().address(ValueOrArray::Array(contracts.clone()));
            watchers = Some((
                handle_interrupt!(self, provider.watch_blocks())??,
                handle_interrupt!(self, provider.watch(&eth_log_filter))??,
            ));

            // sleep for 50ms just to be sure that our watcher is registered and started receiving events
            tokio::time::sleep(Duration::from_millis(50)).await;

            // 4. Check if our LastIncludedEthBlock is same as BestEthBlock
            if best_block == provider.get_block_number().await?
                && best_block_hash
                    == handle_interrupt!(self, provider.get_block(best_block))??
                        .ok_or(RelayerError::InitialSyncAskedForUnknownBlock)?
                        .hash
                        .unwrap()
            {
                // block number and hash are same as before starting watcher over logs.
                // we are safe to continue.
                break;
            }
            // If not the same, stop listening to events and do 2,3,4 steps again.
            // empty pending and do overlapping sync again.
            // Assume this will not happen very often.
        }

        // 5. Continue to active listen on eth events. and prune(commit to db) dequeue for older finalized events
        let finalized_da_height =
            best_block.as_u64() as DaBlockHeight - self.ctx.config.da_finalization();
        self.queue
            .commit_diffs(self.ctx.db.as_mut(), finalized_da_height)
            .await;

        watchers
            .map(|(w1, w2)| (best_block.as_u64() as DaBlockHeight, w1, w2))
            .ok_or_else(|| RelayerError::ProviderError.into())
    }

    /// Starting point of relayer
    #[tracing::instrument(name = "main", skip_all)]
    pub async fn run<P>(mut self, provider: Arc<P>) -> Context
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        self.queue.load_validators(self.ctx.db.as_mut()).await;

        let mut number_of_tries = config::NUMBER_OF_TRIES_FOR_INITIAL_SYNC;
        let (best_block, mut da_blocks_watcher, mut logs_watcher) = loop {
            match self.initial_sync(&provider).await {
                Ok(watcher) => break watcher,
                Err(err) => {
                    if self.status == RelayerStatus::Stop {
                        return self.ctx;
                    }
                    if number_of_tries == 0 {
                        self.status = RelayerStatus::Stop;
                        error!(
                            "Stopping relayer as there are errors on initial sync: {:?}",
                            err
                        );
                        return self.ctx;
                    }
                    error!("Initial sync error:{:?}", err);
                    info!("Number of tries:{:?}", number_of_tries);
                    number_of_tries -= 1;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            };
        };
        info!("Initial syncing finished on block {best_block}. Continue to passive sync.");
        loop {
            tokio::select! {
                inner_fuel_event = self.ctx.receiver.recv() => {
                    if let Some(inner_fuel_event) = inner_fuel_event {
                        self.handle_inner_fuel_event(inner_fuel_event).await;
                    } else {
                        error!("Inner fuel notification broke and returned err");
                        break;
                    }
                }
                fuel_block = self.ctx.new_block_event.recv() => {
                    match fuel_block {
                        Ok(fuel_block) => {
                            self.handle_fuel_block_importer(fuel_block,&provider).await
                        },
                        Err(e) => {
                            error!("Unexpected error happened in relayer new block event requests:{}",e);
                            break;
                        },
                    }
                }
                block_hash = da_blocks_watcher.next() => {
                    if let Some(block_hash) = block_hash {
                        let _ = self.handle_eth_block_hash(&provider,block_hash).await;
                    } else {
                        error!("block watcher closed stream");
                        break;
                    }
                }
                log = logs_watcher.next() => {
                    if let Some(log) = log {
                        self.handle_eth_log(log).await;
                    } else {
                        error!("logs watcher closed stream");
                        self.status = RelayerStatus::Stop;
                        break;
                    }
                }
            }
        }
        self.ctx
    }

    #[tracing::instrument(fields(block.h=block.block().header.height.as_usize(), block.id=block.block().id().to_string().as_str()),skip(self, block, provider))]
    async fn handle_fuel_block_importer<P>(
        &mut self,
        block: ImportBlockBroadcast,
        provider: &Arc<P>,
    ) where
        P: Middleware<Error = ProviderError> + 'static,
    {
        match block {
            ImportBlockBroadcast::PendingFuelBlockImported { .. } => {
                debug!("received new pending fuel block imported event");
            }
            ImportBlockBroadcast::SealedFuelBlockImported {
                block,
                is_created_by_self,
            } => {
                debug!("received new sealed fuel block imported event");
                if is_created_by_self {
                    self.queue
                        .handle_created_fuel_block(&block, self.ctx.db.as_mut(), provider)
                        .await;
                } else {
                    self.queue.handle_fuel_block(&block);
                }
            }
        }
    }

    #[tracing::instrument(skip(self))]
    async fn handle_inner_fuel_event(&mut self, inner_event: RelayerRequest) {
        match inner_event {
            RelayerRequest::Stop => {
                self.status = RelayerStatus::Stop;
            }
            RelayerRequest::GetValidatorSet {
                da_height,
                response,
            } => {
                let res = self
                    .queue
                    .get_validators(da_height, self.ctx.db.as_mut())
                    .await
                    .ok_or(RelayerError::ProviderError);
                let _ = response.send(res);
            }
            RelayerRequest::GetStatus { response } => {
                let _ = response.send(self.status);
            }
        }
    }

    #[tracing::instrument(skip(self, provider, block_hash))]
    async fn handle_eth_block_hash<P>(
        &mut self,
        provider: &P,
        block_hash: H256,
    ) -> Result<(), Error>
    where
        P: Middleware<Error = ProviderError>,
    {
        trace!("Received new block hash:{:x?}", block_hash);
        if let Some(block) = provider.get_block(BlockId::Hash(block_hash)).await? {
            if let Some(da_height) = block.number {
                let finalized_da_height =
                    da_height.as_u64() as DaBlockHeight - self.ctx.config.da_finalization();

                self.queue
                    .commit_diffs(self.ctx.db.as_mut(), finalized_da_height)
                    .await;
            } else {
                error!(
                    "Received block hash does not have block number:block: {:?}",
                    block
                );
            }
        } else {
            error!("received block hash does not exist:{}", block_hash);
        }

        Ok(())
    }

    #[tracing::instrument(skip(self, log))]
    async fn handle_eth_log(&mut self, log: Log) {
        trace!(target:"relayer", "got new log from block:{:?}", log.block_hash);
        self.queue.append_eth_log(log).await;
    }
}

#[cfg(test)]
mod test {

    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use ethers_core::types::{BlockId, BlockNumber, FilterBlockOption, H256, U256, U64};
    use ethers_providers::SyncingStatus;
    use fuel_core_interfaces::{common::fuel_tx::Address, relayer::RelayerRequest};
    use tokio::sync::mpsc;

    use crate::{
        log,
        test_helpers::{relayer, MockData, MockMiddleware, TriggerHandle, TriggerType},
        Config,
    };

    #[tokio::test]
    pub async fn initial_sync_checks_pending_eth_client_and_handling_stop() {
        let config = Config {
            eth_v2_contracts_deployment: 5,
            initial_sync_refresh: Duration::from_millis(10),
            ..Default::default()
        };
        let (relayer, event, _) = relayer(config).await;
        let middle = MockMiddleware::default();
        middle.data.lock().await.is_syncing = SyncingStatus::IsSyncing {
            starting_block: U256::zero(),
            current_block: U256::zero(),
            highest_block: U256::zero(),
        };

        pub struct Handle {
            pub i: u64,
            pub event: mpsc::Sender<RelayerRequest>,
        }
        #[async_trait]
        impl TriggerHandle for Handle {
            async fn run<'a>(&mut self, _: &mut MockData, trigger: TriggerType<'a>) {
                if matches!(trigger, TriggerType::Syncing) {
                    self.i += 1;

                    if self.i == 3 {
                        let _ = self.event.send(RelayerRequest::Stop).await;
                        self.i += 1;
                        return;
                    }
                    if self.i == 4 {
                        panic!("Something is fishy. We should have stopped");
                    }
                } else {
                    panic!("Unknown trigger received");
                }
            }
        }

        middle
            .trigger_handle(Box::new(Handle { i: 0, event }))
            .await;

        relayer.run(Arc::new(middle)).await;
    }

    #[tokio::test]
    pub async fn sync_first_n_finalized_blocks() {
        let config = Config {
            eth_v2_contracts_deployment: 100, // start from block 1
            da_finalization: 30,
            initial_sync_step: 2, // make 2 steps of 2 blocks
            ..Default::default()
        };
        let (relayer, event, _) = relayer(config).await;
        let middle = MockMiddleware::default();
        {
            let mut data = middle.data.lock().await;
            // eth finished syncing
            data.is_syncing = SyncingStatus::IsFalse;
            // best block is 4
            data.best_block.number = Some(U64([134]));
        }
        pub struct Handle {
            pub i: u64,
            pub event: mpsc::Sender<RelayerRequest>,
        }
        #[async_trait]
        impl TriggerHandle for Handle {
            async fn run<'a>(&mut self, _: &mut MockData, trigger: TriggerType<'a>) {
                if let TriggerType::GetLogs(filter) = trigger {
                    if let FilterBlockOption::Range {
                        from_block,
                        to_block,
                    } = filter.block_option
                    {
                        assert_eq!(
                            from_block,
                            Some(BlockNumber::Number(U64([100 + self.i * 2]))),
                            "Start block not matching on i:{:?}",
                            self.i
                        );
                        assert_eq!(
                            to_block,
                            Some(BlockNumber::Number(U64([102 + self.i * 2]))),
                            "Start block not matching on i:{:?}",
                            self.i
                        );
                        self.i += 1;
                    }
                }
                if self.i == 2 {
                    let _ = self.event.send(RelayerRequest::Stop).await;
                    return;
                }
            }
        }
        middle
            .trigger_handle(Box::new(Handle { i: 0, event }))
            .await;

        relayer.run(Arc::new(middle)).await;
    }

    #[tokio::test]
    pub async fn initial_sync() {
        let config = Config {
            eth_v2_contracts_deployment: 100, // start from block 1
            da_finalization: 30,
            initial_sync_step: 2, // make 2 steps of 2 blocks
            ..Default::default()
        };
        let (relayer, event, _) = relayer(config).await;
        let middle = MockMiddleware::default();
        {
            let mut data = middle.data.lock().await;
            // eth finished syncing
            data.is_syncing = SyncingStatus::IsFalse;
            // best block is 4
            data.best_block.number = Some(U64([134]));

            data.best_block.number = Some(U64([134]));
            data.logs_batch = vec![vec![log::tests::eth_log_deposit(
                136,
                Address::zeroed(),
                10,
            )]];
            data.blocks_batch = vec![vec![H256::zero()]];
        }
        pub struct Handle {
            pub i: u64,
            pub event: mpsc::Sender<RelayerRequest>,
        }
        #[async_trait]
        impl TriggerHandle for Handle {
            async fn run<'a>(&mut self, _: &mut MockData, trigger: TriggerType<'a>) {
                match self.i {
                    // check if eth client is in sync.
                    0 => assert_eq!(
                        TriggerType::Syncing,
                        trigger,
                        "We need to check if eth client is synced"
                    ),
                    // get best eth block number so that we know until when to sync
                    1 => assert_eq!(
                        TriggerType::GetBlockNumber,
                        trigger,
                        "We need to get Best eth block number"
                    ),
                    // get first batch of logs.
                    2 => match trigger {
                        TriggerType::GetLogs(filter) => {
                            match filter.block_option {
                                FilterBlockOption::Range {
                                    from_block,
                                    to_block,
                                } => {
                                    assert_eq!(from_block, Some(BlockNumber::Number(U64([100]))));
                                    assert_eq!(to_block, Some(BlockNumber::Number(U64([102]))));
                                }
                                _ => panic!("Expect filter block option range"),
                            };
                        }
                        _ => panic!("wrong trigger:{:?} we expected get logs 1", trigger),
                    },
                    // get second batch of logs. for initial sync
                    3 => match trigger {
                        TriggerType::GetLogs(filter) => {
                            match filter.block_option {
                                FilterBlockOption::Range {
                                    from_block,
                                    to_block,
                                } => {
                                    assert_eq!(from_block, Some(BlockNumber::Number(U64([102]))));
                                    assert_eq!(to_block, Some(BlockNumber::Number(U64([104]))));
                                }
                                _ => panic!("Expect filter block option range"),
                            };
                        }
                        _ => panic!("wrong trigger:{:?} we expected get logs 1", trigger),
                    },
                    // update our best block
                    4 => {
                        assert_eq!(
                            TriggerType::GetBlockNumber,
                            trigger,
                            "We need to get Best eth block number again"
                        )
                    }
                    // get block hash from best block number
                    5 => {
                        assert_eq!(
                            TriggerType::GetBlock(BlockId::Number(BlockNumber::Number(U64([134])))),
                            trigger,
                            "Get block hash from best block number"
                        )
                    }
                    // get block log from current finalized to best block
                    6 => match trigger {
                        TriggerType::GetLogs(filter) => {
                            match filter.block_option {
                                FilterBlockOption::Range {
                                    from_block,
                                    to_block,
                                } => {
                                    assert_eq!(from_block, Some(BlockNumber::Number(U64([104]))));
                                    assert_eq!(to_block, Some(BlockNumber::Number(U64([134]))));
                                }
                                _ => panic!("Expect filter block option range for 6"),
                            };
                        }
                        _ => panic!("wrong trigger:{:?} we expected get logs 6", trigger),
                    },
                    // get best eth block to synchronize log watcher
                    7 => {
                        assert_eq!(
                            TriggerType::GetBlockNumber,
                            trigger,
                            "We need to get Best eth block number to check that it is not changed"
                        )
                    }
                    // get best eth block hash to synchronize log watcher
                    8 => {
                        assert_eq!(
                            TriggerType::GetBlock(BlockId::Number(BlockNumber::Number(U64([134])))),
                            trigger,
                            "Get block hash from best block number to check that it is not changed"
                        )
                    }
                    _ => panic!("Unknown request, we should have finished until now"),
                }
                self.i += 1;
            }
        }
        middle
            .trigger_handle(Box::new(Handle { i: 0, event }))
            .await;

        relayer.run(Arc::new(middle)).await;
    }
}
