use std::{
    cmp::{max, min},
    time::Duration,
};

use crate::{
    log::EthEventLog,
    pending_events::{PendingDiff, PendingEvents},
    validator_set::CurrentValidatorSet,
    Config,
};
use futures::Future;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, trace, warn};

use anyhow::Error;
use ethers_core::types::{Filter, Log, ValueOrArray};
use ethers_providers::{
    FilterWatcher, Middleware, Provider, ProviderError, StreamExt, SyncingStatus, Ws,
};
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    relayer::{RelayerDB, RelayerError, RelayerEvent, RelayerStatus},
    signer::Signer,
};
///
pub struct Relayer {
    /// Pendning stakes/assets/withdrawals. Before they are finalized
    pending: PendingEvents,
    /// Current validator set
    current_validator_set: CurrentValidatorSet,
    /// db connector to apply stake and token deposit
    db: Box<Mutex<dyn RelayerDB>>,
    /// Relayer Configuration
    config: Config,
    /// state of relayer
    status: RelayerStatus,
    /// new fuel block notifier.
    receiver: mpsc::Receiver<RelayerEvent>,
    /// Notification of new block event
    new_block_event: broadcast::Receiver<NewBlockEvent>,
    /// Service for signing of arbitrary hash
    _signer: Box<dyn Signer + Send>,
}

impl Relayer {
    pub fn new(
        config: Config,
        db: Box<Mutex<dyn RelayerDB>>,
        receiver: mpsc::Receiver<RelayerEvent>,
        new_block_event: broadcast::Receiver<NewBlockEvent>,
        signer: Box<dyn Signer + Send>,
    ) -> Self {
        Self {
            config,
            db,
            pending: PendingEvents::new(),
            current_validator_set: CurrentValidatorSet::new(),
            status: RelayerStatus::EthIsSyncing,
            receiver,
            new_block_event,
            _signer: signer,
        }
    }

    /// create provider that we use for communication with ethereum.
    pub async fn provider(uri: &str) -> Result<Provider<Ws>, Error> {
        let ws = Ws::connect(uri).await?;
        let provider = Provider::new(ws);
        Ok(provider)
    }

    // /// Used in two places. On initial sync and when new fuel blocks is
    // async fn apply_last_validator_diff(&mut self, current_eth_number: u64) {
    //     let finalized_eth_block = current_eth_number - self.config.eth_finality_slider();
    //     while let Some(diffs) = self.pending.back() {
    //         if diffs.eth_number < finalized_eth_block {
    //             break;
    //         }
    //         let mut stake_diff = HashMap::new();
    //         // apply diff to validator_set
    //         for (address, diff) in &diffs.stake_diff {
    //             let value = self.finalized_validator_set.entry(*address).or_insert(0);
    //             // we are okay to cast it, we dont expect that big of number to exist.
    //             *value = ((*value as i64) + diff) as u64;
    //             stake_diff.insert(*address, *value);
    //         }
    //         // push new value for changed validators to database
    //         self.db
    //             .lock()
    //             .await
    //             .insert_validator_set_diff(diffs.fuel_number, &stake_diff)
    //             .await;
    //         self.db
    //             .lock()
    //             .await
    //             .set_fuel_finalized_block(diffs.fuel_number)
    //             .await;

    //         // push fanalized deposit to db
    //         let block_enabled_fuel_block = diffs.fuel_number + self.config.fuel_finality_slider();
    //         for (nonce, deposit) in diffs.assets_deposited.iter() {
    //             self.db
    //                 .lock()
    //                 .await
    //                 .insert_token_deposit(
    //                     *nonce,
    //                     block_enabled_fuel_block,
    //                     deposit.0,
    //                     deposit.1,
    //                     deposit.2,
    //                 )
    //                 .await
    //         }
    //         self.db
    //             .lock()
    //             .await
    //             .set_eth_finalized_block(finalized_eth_block)
    //             .await;
    //         self.finalized_fuel_block = diffs.fuel_number;
    //         self.pending.pop_back();
    //     }
    //     self.db
    //         .lock()
    //         .await
    //         .set_eth_finalized_block(finalized_eth_block)
    //         .await;
    // }

    async fn stop_handle<T: Future>(
        &mut self,
        handle: impl Fn() -> T,
    ) -> Result<T::Output, RelayerError> {
        loop {
            tokio::select! {
                biased;
                inner_fuel_event = self.receiver.recv() => {
                    println!("Received fuel event");
                    match inner_fuel_event.unwrap() {
                        RelayerEvent::Stop=>{
                            self.status = RelayerStatus::Stop;
                            return Err(RelayerError::Stoped.into());
                        },
                        RelayerEvent::GetValidatorSet {response_channel, .. } => {
                            let _ = response_channel.send(Err(RelayerError::ValidatorSetEthClientSyncing));
                        },
                        RelayerEvent::GetStatus { response } => {
                            let _ = response.send(self.status);
                        },
                    }
                }
                o = handle() => {
                    return Ok(o)
                }
            }
        }
    }

    /// Initial syncing from ethereum logs into fuel database. It does overlapping syncronization and returns
    /// logs watcher with assurence that we didnt miss any events.
    async fn inital_sync<'a, P>(
        &mut self,
        provider: &'a P,
    ) -> Result<FilterWatcher<'a, P::Provider, Log>, Error>
    where
        P: Middleware<Error = ProviderError>,
    {
        // loop and wait for eth client to finish syncing
        loop {
            if matches!(
                self.stop_handle(|| provider.syncing()).await??,
                SyncingStatus::IsFalse
            ) {
                break;
            }
            self.stop_handle(|| tokio::time::sleep(Duration::from_secs(5)))
                .await?;
        }

        let last_finalized_eth_block = std::cmp::max(
            self.config.eth_v2_contract_deployment(),
            self.db.lock().await.get_eth_finalized_block().await,
        );
        // should be allways more then last finalized_eth_blocks

        let best_finalized_block =
            provider.get_block_number().await?.as_u64() - self.config.eth_finality_slider();

        // 1. sync from HardCoddedContractCreatingBlock->BestEthBlock-100)
        let step = self.config.initial_sync_step(); // do some stats on optimal value
        let contracts = self.config.eth_v2_contract_addresses().to_vec();
        // on start of contract there is possibility of them being overlapping, so we want to skip for loop
        // with next line
        let best_finalized_block = max(last_finalized_eth_block, best_finalized_block);
        for start in (last_finalized_eth_block..best_finalized_block).step_by(step) {
            let end = min(start + step as u64, best_finalized_block);

            // TODO  can be parallelized
            let filter = Filter::new()
                .from_block(start)
                .to_block(end)
                .address(ValueOrArray::Array(contracts.clone()));
            let logs = self.stop_handle(|| provider.get_logs(&filter)).await??;

            for eth_event in logs {
                let fuel_event = EthEventLog::try_from(&eth_event);
                if let Err(err) = fuel_event {
                    // not formated event from contract
                    // just skip it for now.
                    continue;
                }
                let fuel_event = fuel_event.unwrap();
                self.pending
                    .append_eth_events(&fuel_event, eth_event.block_number.unwrap().as_u64())
                    .await;
            }
            // if there is more then two items in pending list flush first one.
            // Having two elements in this stage means that full fuel block is already processed and
            // we dont have reverts to dispute that.
            while self.pending.len() > 1 {
                // we are sending dummy eth block bcs we are sure that it is finalized
                self.pending
                    .apply_last_validator_diff(
                        &mut *self.db.lock().await,
                        u64::MAX,
                    )
                    .await;
            }
        }

        // if there is no diffs it means we are at start of contract creating
        let last_diff = if self.pending.is_empty() {
            // set fuel num to zero and contract creating eth.
            PendingDiff::new(0)
        } else {
            // apply all pending changed.
            while self.pending.len() > 1 {
                // we are sending dummy eth block num bcs we are sure that it is finalized
                self.pending
                    .apply_last_validator_diff(
                        &mut *self.db.lock().await,
                        u64::MAX,
                    )
                    .await;
            }
            self.pending.pop_front().unwrap()
        };

        // TODO probably not needed now. but after some time we will need to do sync to best block here.
        // it depends on how much time it is needed to tranverse first part of this function
        // and how much lag happened in meantime.

        let mut watcher: Option<FilterWatcher<_, _>>;
        let last_included_block = best_finalized_block;

        let mut best_block;

        loop {
            // 1. get best block and its hash sync over it, and push it over
            self.pending.clear();
            self.pending.push_front(last_diff.clone());

            best_block = self.stop_handle(|| provider.get_block_number()).await??;
            // there is not get block latest from ethers so we need to do it in two steps to get hash

            let block = self
                .stop_handle(|| provider.get_block(best_block))
                .await??
                .ok_or(RelayerError::InitialSyncAskedForUnknownBlock)?;
            let best_block_hash = block.hash.unwrap(); // it is okay to unwrap

            // 2. sync overlap from LastIncludedEthBlock-> BestEthBlock) they are saved in dequeue.
            let filter = Filter::new()
                .from_block(last_included_block)
                .to_block(best_block)
                .address(ValueOrArray::Array(contracts.clone()));

            let logs = self.stop_handle(|| provider.get_logs(&filter)).await??;

            for eth_event in logs {
                let fuel_event = EthEventLog::try_from(&eth_event);
                if let Err(err) = fuel_event {
                    // not formated event from contract
                    error!(target:"relayer", "Eth Event not formated properly in inital sync:{}",err);
                    // just skip it for now.
                    continue;
                }
                let fuel_event = fuel_event.unwrap();
                self.pending
                    .append_eth_events(&fuel_event, eth_event.block_number.unwrap().as_u64())
                    .await;
            }

            // 3. Start listening to eth events
            let eth_log_filter = Filter::new().address(ValueOrArray::Array(contracts.clone()));
            watcher = Some(
                self.stop_handle(|| provider.watch(&eth_log_filter))
                    .await??,
            );

            //let t = watcher.as_mut().expect(()).next().await;
            // sleep for 50ms just to be sure that our watcher is registered and started receiving events
            tokio::time::sleep(Duration::from_millis(50)).await;

            // 4. Check if our LastIncludedEthBlock is same as BestEthBlock
            if best_block == provider.get_block_number().await?
                && best_block_hash
                    == self
                        .stop_handle(|| provider.get_block(best_block))
                        .await??
                        .ok_or(RelayerError::InitialSyncAskedForUnknownBlock)?
                        .hash
                        .unwrap()
            {
                // block number and hash are same as before starting watcher over logs.
                // we are safe to continue.
                break;
            }
            // If not the same, stop listening to events and do 2,3,4 steps again.
            // empty pending and do overlaping sync again.
            // Assume this will not happen very often.
            self.pending.clear();
        }

        // 5. Continue to active listen on eth events. and prune(commit to db) dequeue for older finalized events
        while self.pending.len() > self.config.fuel_finality_slider() as usize {
            self.pending
                .apply_last_validator_diff(
                    &mut *self.db.lock().await,
                    best_block.as_u64(),
                )
                .await;
        }

        watcher.ok_or(RelayerError::ProviderError.into())
    }

    /// Starting point of relayer
    pub async fn run<P>(self, provider: P)
    where
        P: Middleware<Error = ProviderError>,
    {
        let mut this = self;
        this.current_validator_set
            .load_current_validator_set(&*this.db.lock().await)
            .await;

        loop {
            // initial sync
            let mut logs_watcher = match this.inital_sync(&provider).await {
                Ok(watcher) => watcher,
                Err(_err) => {
                    if this.status == RelayerStatus::Stop {
                        return;
                    }
                    continue;
                }
            };

            println!("Finished initial sync");
            loop {
                tokio::select! {
                    inner_fuel_event = this.receiver.recv() => {
                        println!("Get Inner event:{:?}",inner_fuel_event);
                        if inner_fuel_event.is_none() {
                            error!("Inner fuel notification broke and returned err");
                            this.status = RelayerStatus::Stop;
                            return;
                        }
                        this.handle_inner_fuel_event(inner_fuel_event.unwrap()).await;
                    }

                    new_block = this.new_block_event.recv() => {
                        println!("Get new_block event:{:?}",new_block);
                        match new_block {
                            Err(e) => {
                                error!("Unexpected error happened in relayer new block event receiver:{}",e);
                                return;
                            },
                            Ok(new_block) => {
                                this.handle_new_block_event(new_block).await
                            },
                        }
                    }
                    log = logs_watcher.next() => {
                        println!("Get log event:{:?}",log);
                        this.handle_eth_event(log).await
                    }
                }
            }
        }
    }

    async fn handle_new_block_event(&mut self, new_block: NewBlockEvent) {
        match new_block {
            NewBlockEvent::NewBlockCreated(_created_block) => {
                // TODO:
                // 1. compress block for eth contract.
                // 2. Create eth transaction.
                // 3. Sign transaction
                // 4. Send transaction to eth client.
            }
            NewBlockEvent::NewBlockIncluded { height, eth_height } => {
                // assume that eth_height is checked agains parent block.

                // ignore reorganization
                let finality_slider = self.config.fuel_finality_slider();

                // TODO handle lagging here. compare current_fuel_block and finalized_fuel_block and send error notification
                // if we are lagging over ethereum events.

                // first if is for start of contract and first few validator blocks
                    self.current_validator_set
                        .bump_set_to_the_block(
                            eth_height,
                            &*self.db.lock().await,
                        ).await
            }
        }
    }

    async fn handle_inner_fuel_event(&mut self, inner_event: RelayerEvent) {
        match inner_event {
            RelayerEvent::Stop => {
                self.status = RelayerStatus::Stop;
            }
            RelayerEvent::GetValidatorSet {
                fuel_block,
                response_channel,
            } => {
                let finality_slider = self.config.fuel_finality_slider();
                let validator_set_block =
                    std::cmp::max(fuel_block, finality_slider) - finality_slider;

                let res = match self
                    .current_validator_set
                    .get_validator_set(validator_set_block)
                {
                    Some(set) => Ok(set),
                    None => Err(RelayerError::ProviderError),
                };
                let _ = response_channel.send(res);
            }
            RelayerEvent::GetStatus { response } => {
                let _ = response.send(self.status.clone());
            }
        }
    }

    async fn handle_eth_event(&mut self, eth_event: Option<Log>) {
        // new log
        if eth_event.is_none() {
            // TODO make proper reconnect options.
            warn!("We broke something. Set state to not eth not connected and do retry");
            return;
        }
        let eth_event = eth_event.unwrap();
        trace!(target:"relayer", "got new log:{:?}", eth_event.block_hash);
        let fuel_event = EthEventLog::try_from(&eth_event);
        if let Err(err) = fuel_event {
            warn!(target:"relayer", "Eth Event not formated properly:{}",err);
            return;
        }
        if eth_event.block_number.is_none() {
            error!(target:"relayer", "Block number not found in eth log");
            return;
        }
        if eth_event.removed.is_none() {
            error!(target:"relayer", "Remove not found in eth log");
            return;
        }
        let removed = eth_event.removed.unwrap();
        let block_number = eth_event.block_number.unwrap().as_u64();
        let fuel_event = fuel_event.unwrap();

        self.pending
            .handle_eth_event(fuel_event, block_number, removed)
            .await;

        // apply pending eth diffs
        let finalized_eth_height = block_number-self.config.eth_finality_slider;
        self.pending.apply_last_validator_diff(&mut *self.db.lock().await, finalized_eth_height).await;
    }
}

#[cfg(test)]
mod test {

    use async_trait::async_trait;
    use ethers_core::types::{BlockId, BlockNumber, FilterBlockOption, U256, U64};
    use ethers_providers::SyncingStatus;
    use fuel_core_interfaces::relayer::RelayerEvent;
    use fuel_tx::Address;
    use tokio::sync::mpsc;

    use crate::{
        log,
        test::{relayer, MockData, MockMiddleware, TriggerHandle, TriggerType},
        Config,
    };

    #[tokio::test]
    pub async fn initial_sync_checks_eth_client_pending_and_handling_stop() {
        let mut config = Config::new();
        config.eth_v2_contract_deployment = 5;
        let (relayer, event, _) = relayer(config);
        let middle = MockMiddleware::new();
        middle.data.lock().await.is_syncing = SyncingStatus::IsSyncing {
            starting_block: U256::zero(),
            current_block: U256::zero(),
            highest_block: U256::zero(),
        };

        pub struct Handle {
            pub i: u64,
            pub event: mpsc::Sender<RelayerEvent>,
        }
        #[async_trait]
        impl TriggerHandle for Handle {
            async fn run(&mut self, _: &mut MockData, trigger: TriggerType) {
                if matches!(trigger, TriggerType::Syncing) {
                    self.i += 1;

                    if self.i == 3 {
                        let _ = self.event.send(RelayerEvent::Stop).await;
                        self.i += 1;
                        return;
                    }
                    if self.i == 4 {
                        assert!(true, "Something is fishy. We should have stopped");
                    }
                } else {
                    assert!(true, "Unknown trigger received");
                }
            }
        }

        middle
            .trigger_handle(Box::new(Handle { i: 0, event }))
            .await;

        relayer.run(middle).await;
    }

    #[tokio::test]
    pub async fn sync_first_n_finalized_blocks() {
        let mut config = Config::new();
        // start from block 1
        config.eth_v2_contract_deployment = 100;
        config.eth_finality_slider = 30;
        // make 2 steps of 2 blocks
        config.initial_sync_step = 2;
        let (relayer, event, _) = relayer(config);
        let middle = MockMiddleware::new();
        {
            let mut data = middle.data.lock().await;
            // eth finished syncing
            data.is_syncing = SyncingStatus::IsFalse;
            // best block is 4
            data.best_block.number = Some(U64([134]));
        }
        pub struct Handle {
            pub i: u64,
            pub event: mpsc::Sender<RelayerEvent>,
        }
        #[async_trait]
        impl TriggerHandle for Handle {
            async fn run(&mut self, _: &mut MockData, trigger: TriggerType) {
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
                    let _ = self.event.send(RelayerEvent::Stop).await;
                    return;
                }
            }
        }
        middle
            .trigger_handle(Box::new(Handle { i: 0, event }))
            .await;

        relayer.run(middle).await;
    }

    #[tokio::test]
    pub async fn initial_sync() {
        let mut config = Config::new();
        // start from block 1
        config.eth_v2_contract_deployment = 100;
        config.eth_finality_slider = 30;
        // make 2 steps of 2 blocks
        config.initial_sync_step = 2;
        let (relayer, event, _) = relayer(config);
        let middle = MockMiddleware::new();
        {
            let mut data = middle.data.lock().await;
            // eth finished syncing
            data.is_syncing = SyncingStatus::IsFalse;
            // best block is 4
            data.best_block.number = Some(U64([134]));
        }
        pub struct Handle {
            pub i: u64,
            pub event: mpsc::Sender<RelayerEvent>,
        }
        #[async_trait]
        impl TriggerHandle for Handle {
            async fn run(&mut self, _: &mut MockData, trigger: TriggerType) {
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
                                _ => assert!(true, "Expect filter block option range"),
                            };
                        }
                        _ => assert!(true, "wrong trigger:{:?} we expected get logs 1", trigger),
                    },
                    // get second batch of logs. for initialy sync
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
                                _ => assert!(true, "Expect filter block option range"),
                            };
                        }
                        _ => assert!(true, "wrong trigger:{:?} we expected get logs 1", trigger),
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
                                _ => assert!(true, "Expect filter block option range for 6"),
                            };
                        }
                        _ => assert!(true, "wrong trigger:{:?} we expected get logs 6", trigger),
                    },
                    // get best eth block to syncornize log watcher
                    7 => {
                        assert_eq!(
                            TriggerType::GetBlockNumber,
                            trigger,
                            "We need to get Best eth block number to check that it is not changed"
                        )
                    }
                    // get best eth block hash to syncronize log watcher
                    8 => {
                        assert_eq!(
                            TriggerType::GetBlock(BlockId::Number(BlockNumber::Number(U64([134])))),
                            trigger,
                            "Get block hash from best block number to check that it is not changed"
                        )
                    }
                    _ => assert!(true, "Unknown request, we should have finished until now"),
                }
                self.i += 1;
            }
        }
        middle
            .trigger_handle(Box::new(Handle { i: 0, event }))
            .await;

        relayer.run(middle).await;
    }

    #[tokio::test]
    pub async fn passive_sync() {
        let mut config = Config::new();
        // start from block 1
        config.eth_v2_contract_deployment = 100;
        config.eth_finality_slider = 30;
        // make 2 steps of 2 blocks
        config.initial_sync_step = 2;
        let (relayer, event, new_block_event) = relayer(config);
        let middle = MockMiddleware::new();
        {
            let mut data = middle.data.lock().await;
            // eth finished syncing
            data.is_syncing = SyncingStatus::IsFalse;
            // best block is 4
            data.best_block.number = Some(U64([134]));
            let log1 = data.logs_batch = vec![
                vec![log::tests::eth_log_validator_deposit(
                    136,
                    Address::zeroed(),
                    10,
                )], //Log::]
            ];
        }
        pub struct Handle {
            pub i: u64,
            pub event: mpsc::Sender<RelayerEvent>,
        }
        #[async_trait]
        impl TriggerHandle for Handle {
            async fn run(&mut self, data: &mut MockData, trigger: TriggerType) {
                //println!("TEST{:?}   type:{:?}", self.i, trigger);
                match self.i {
                    n if n < 9 => (), // initialsync, skip it.
                    9 => data.logs_batch_index += 1,
                    10 => (),
                    11 => (),
                    _ => assert!(true, "Case not covered"),
                }
                self.i += 1;
            }
        }
        middle
            .trigger_handle(Box::new(Handle { i: 0, event }))
            .await;

        relayer.run(middle).await;
    }
}
