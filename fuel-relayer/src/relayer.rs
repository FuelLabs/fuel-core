use std::{
    cmp::{max, min},
    collections::{HashMap, VecDeque},
    time::Duration,
};

use crate::{log::EthEventLog, Config};
use fuel_types::{Address, Bytes32, Color, Word};
use futures::Future;
use log::{error, info, trace, warn};
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};

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
    pending: VecDeque<PendingDiff>,
    /// finalized validator set
    finalized_validator_set: HashMap<Address, u64>,
    /// finalized fuel block
    finalized_fuel_block: u64,
    /// Current validator set
    current_validator_set: HashMap<Address, u64>,
    /// current fuel block
    current_fuel_block: u64,
    /// db connector to apply stake and token deposit
    db: Box<Mutex<dyn RelayerDB>>,
    /// Relayer Configuration
    config: Config,
    /// state of relayer
    status: RelayerStatus,
    /// new fuel block notifier.
    receiver: mpsc::Receiver<RelayerEvent>,
    /// This is litlle bit hacky but because we relate validator staking with fuel commit block and not on eth block
    /// we need to be sure that we are taking proper order of those transactions
    /// Revert are reported as list of reverted logs in order of Block2Log1,Block2Log2,Block1Log1,Block2Log2.
    /// I checked this with infura endpoint.
    pending_removed_eth_events: Vec<(u64, Vec<EthEventLog>)>,
    /// Notification of new block event
    new_block_event: broadcast::Receiver<NewBlockEvent>,
    /// Service for signing of arbitrary hash
    _signer: Box<dyn Signer + Send>,
}

/// Pending diff between FuelBlocks
#[derive(Clone, Debug)]
pub struct PendingDiff {
    /// fuel block number,
    fuel_number: u64,
    /// eth block number, Represent when child number got included in what block.
    /// This means that when that block is finalized we are okay to commit this pending diff.
    eth_number: u64,
    /// Validator stake deposit and withdrawel.
    stake_diff: HashMap<Address, i64>,
    /// erc-20 pending deposit. deposit nonce.
    assets_deposited: HashMap<Bytes32, (Address, Color, Word)>,
}

impl PendingDiff {
    pub fn new(fuel_number: u64) -> Self {
        Self {
            fuel_number,
            eth_number: u64::MAX,
            stake_diff: HashMap::new(),
            assets_deposited: HashMap::new(),
        }
    }
    pub fn stake_diff(&self) -> &HashMap<Address, i64> {
        &self.stake_diff
    }
    pub fn assets_deposited(&self) -> &HashMap<Bytes32, (Address, Color, Word)> {
        &self.assets_deposited
    }
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
            pending: VecDeque::new(),
            finalized_validator_set: HashMap::new(),
            finalized_fuel_block: 0,
            current_validator_set: HashMap::new(),
            current_fuel_block: 0,
            status: RelayerStatus::EthIsSyncing,
            receiver,
            new_block_event,
            _signer: signer,
            pending_removed_eth_events: Vec::new(),
        }
    }

    /// create provider that we use for communication with ethereum.
    pub async fn provider(uri: &str) -> Result<Provider<Ws>, Error> {
        let ws = Ws::connect(uri).await?;
        let provider = Provider::new(ws);
        Ok(provider)
    }

    /// Used in two places. On initial sync and when new fuel blocks is
    async fn apply_last_validator_diff(&mut self, current_eth_number: u64) {
        let finalized_eth_block = current_eth_number - self.config.eth_finality_slider();
        while let Some(diffs) = self.pending.back() {
            if diffs.eth_number < finalized_eth_block {
                break;
            }
            let mut stake_diff = HashMap::new();
            // apply diff to validator_set
            for (address, diff) in &diffs.stake_diff {
                let value = self.finalized_validator_set.entry(*address).or_insert(0);
                // we are okay to cast it, we dont expect that big of number to exist.
                *value = ((*value as i64) + diff) as u64;
                stake_diff.insert(*address, *value);
            }
            // push new value for changed validators to database
            self.db
                .lock()
                .await
                .insert_validator_set_diff(diffs.fuel_number, &stake_diff)
                .await;
            self.db
                .lock()
                .await
                .set_fuel_finalized_block(diffs.fuel_number)
                .await;

            // push fanalized deposit to db
            let block_enabled_fuel_block = diffs.fuel_number + self.config.fuel_finality_slider();
            for (nonce, deposit) in diffs.assets_deposited.iter() {
                self.db
                    .lock()
                    .await
                    .insert_token_deposit(
                        *nonce,
                        block_enabled_fuel_block,
                        deposit.0,
                        deposit.1,
                        deposit.2,
                    )
                    .await
            }
            self.db
                .lock()
                .await
                .set_eth_finalized_block(finalized_eth_block)
                .await;
            self.finalized_fuel_block = diffs.fuel_number;
            self.pending.pop_back();
        }
        self.db
            .lock()
            .await
            .set_eth_finalized_block(finalized_eth_block)
            .await;
    }

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
        println!("Wait client wait loop finished");

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
        println!(
            "start{}..end{}",
            last_finalized_eth_block, best_finalized_block
        );
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
                    error!(target:"relayer", "Eth Event not formated properly in inital sync:{}",err);
                    // just skip it for now.
                    continue;
                }
                let fuel_event = fuel_event.unwrap();
                self.append_eth_events(&fuel_event, eth_event.block_number.unwrap().as_u64())
                    .await;
            }
            // if there is more then two items in pending list flush first one.
            // Having two elements in this stage means that full fuel block is already processed and
            // we dont have reverts to dispute that.
            while self.pending.len() > 1 {
                // we are sending dummy eth block bcs we are sure that it is finalized
                self.apply_last_validator_diff(u64::MAX).await;
            }
        }

        println!("Initial load for loop finished");

        // if there is no diffs it means we are at start of contract creating
        let last_diff = if self.pending.is_empty() {
            // set fuel num to zero and contract creating eth.
            PendingDiff::new(0)
        } else {
            // apply all pending changed.
            while self.pending.len() > 1 {
                // we are sending dummy eth block num bcs we are sure that it is finalized
                self.apply_last_validator_diff(u64::MAX).await;
            }
            self.pending.pop_front().unwrap()
        };

        // TODO probably not needed now. but after some time we will need to do sync to best block here.
        // it depends on how much time it is needed to tranverse first part of this function
        // and how much lag happened in meantime.

        let mut watcher: Option<FilterWatcher<_, _>>;
        let last_included_block = best_finalized_block;

        let mut best_block;

        println!("Start intermediate sync loop");
        loop {
            // 1. get best block and its hash sync over it, and push it over
            println!("test one");
            self.pending.clear();
            self.pending.push_front(last_diff.clone());

            best_block = self.stop_handle(|| provider.get_block_number()).await??;
            // there is not get block latest from ethers so we need to do it in two steps to get hash

            let block = self
                .stop_handle(|| provider.get_block(best_block))
                .await??
                .ok_or(RelayerError::InitialSyncAskedForUnknownBlock)?;
            let best_block_hash = block.hash.unwrap(); // it is okay to unwrap
            println!("test two");

            // 2. sync overlap from LastIncludedEthBlock-> BestEthBlock) they are saved in dequeue.
            let filter = Filter::new()
                .from_block(last_included_block)
                .to_block(best_block)
                .address(ValueOrArray::Array(contracts.clone()));

            let logs = self.stop_handle(|| provider.get_logs(&filter)).await??;

            println!("test threa");

            for eth_event in logs {
                let fuel_event = EthEventLog::try_from(&eth_event);
                if let Err(err) = fuel_event {
                    // not formated event from contract
                    error!(target:"relayer", "Eth Event not formated properly in inital sync:{}",err);
                    // just skip it for now.
                    continue;
                }
                let fuel_event = fuel_event.unwrap();
                self.append_eth_events(&fuel_event, eth_event.block_number.unwrap().as_u64())
                    .await;
            }

            // 3. Start listening to eth events
            let eth_log_filter = Filter::new().address(ValueOrArray::Array(contracts.clone()));
            watcher = Some(
                self.stop_handle(|| provider.watch(&eth_log_filter))
                    .await??,
            );
            println!("test 4th");

            //let t = watcher.as_mut().expect(()).next().await;
            // sleep for 50ms just to be sure that our watcher is registered and started receiving events
            tokio::time::sleep(Duration::from_millis(50)).await;

            println!("test 5th");

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
                println!("Break intermediate loop");
                // block number and hash are same as before starting watcher over logs.
                // we are safe to continue.
                break;
            }
            // If not the same, stop listening to events and do 2,3,4 steps again.
            // empty pending and do overlaping sync again.
            // Assume this will not happen very often.
            println!("Need to do overlaping sync again");
            self.pending.clear();
        }

        // 5. Continue to active listen on eth events. and prune(commit to db) dequeue for older finalized events
        while self.pending.len() > self.config.fuel_finality_slider() as usize {
            self.apply_last_validator_diff(best_block.as_u64()).await;
        }

        watcher.ok_or(RelayerError::ProviderError.into())
    }

    // probably not going to metter a lot we expect for validator stake to be mostly unchanged.
    // TODO in becomes troublesome to load and takes a lot of time, it is good to optimize
    async fn load_current_validator_set(&mut self, best_fuel_block: u64) {
        let mut validator_set = HashMap::new();
        for (_, diffs) in self
            .db
            .lock()
            .await
            .get_validator_set_diff(0, Some(best_fuel_block))
            .await
        {
            validator_set.extend(diffs)
        }
        self.current_fuel_block = best_fuel_block;
    }

    /// Starting point of relayer
    pub async fn run<P>(self, provider: P, best_fuel_block: u64)
    where
        P: Middleware<Error = ProviderError>,
    {
        let mut this = self;

        // iterate over validator sets and update it to best_fuel_block.
        this.load_current_validator_set(best_fuel_block).await;

        loop {
            let mut logs_watcher = match this.inital_sync(&provider).await {
                Ok(watcher) => watcher,
                Err(err) => {
                    println!("Initial sync error:{}, try again", err);
                    if this.status == RelayerStatus::Stop {
                        println!("Stop return.");
                        return;
                    }
                    continue;
                }
            };
            println!("END1");

            if this.status == RelayerStatus::Stop {
                println!("Stop return2\n");
                return;
            }

            tokio::select! {
                inner_fuel_event = this.receiver.recv() => {
                    if inner_fuel_event.is_none() {
                        error!("Inner fuel notification broke and returned err");
                        this.status = RelayerStatus::Stop;
                    }
                    this.handle_inner_fuel_event(inner_fuel_event.unwrap()).await;
                }

                new_block = this.new_block_event.recv() => {
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
                    this.handle_eth_event(log).await
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
            NewBlockEvent::NewBlockIncluded(block_number) => {
                // ignore reorganization

                let finality_slider = self.config.fuel_finality_slider();
                let validator_set_block =
                    std::cmp::max(block_number, finality_slider) - finality_slider;

                // TODO handle lagging here. compare current_fuel_block and finalized_fuel_block and send error notification
                // if we are lagging over ethereum events.

                // first if is for start of contract and first few validator blocks
                if validator_set_block < finality_slider {
                    if self.current_fuel_block != 0 {
                        error!( "Initial sync seems incorrent. current_fuel_block should be zero but it is {}", self.current_fuel_block);
                    }
                    return;
                } else {
                    // we assume that for every new fuel block number is increments by one
                    let new_current_fuel_block = self.current_fuel_block + 1;
                    if new_current_fuel_block != validator_set_block {
                        error!("Inconsistency in new fuel block new validator set block is {} but our current is {}",validator_set_block,self.current_fuel_block);
                    }
                    let mut db_changes = self
                        .db
                        .lock()
                        .await
                        .get_validator_set_diff(
                            new_current_fuel_block,
                            Some(new_current_fuel_block),
                        )
                        .await;
                    if let Some((_, changes)) = db_changes.pop() {
                        for (address, new_value) in changes {
                            self.current_validator_set.insert(address, new_value);
                        }
                    }
                    self.current_fuel_block = new_current_fuel_block;
                }
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

                let validators = if validator_set_block == self.current_fuel_block {
                    // if we are asking current validator set just return them.
                    // In first impl this is only thing we will need.
                    Ok(self.current_validator_set.clone())
                } else if validator_set_block > self.finalized_fuel_block {
                    // we are lagging over ethereum finalization
                    warn!("We started lagging over eth finalization");
                    Err(RelayerError::ProviderError)
                } else {
                    //TODO make this available for all validator sets, go over db and apply diffs between them.
                    // for first iteration it is not needed.
                    Err(RelayerError::ProviderError)
                };
                let _ = response_channel.send(validators);
            }
            RelayerEvent::GetStatus { response } => {
                let _ = response.send(self.status.clone());
            }
        }
    }

    async fn revert_eth_event(&mut self, fuel_event: &EthEventLog) {
        match *fuel_event {
            EthEventLog::AssetDeposit { deposit_nonce, .. } => {
                if let Some(pending) = self.pending.front_mut() {
                    pending.assets_deposited.remove(&deposit_nonce);
                }
            }
            EthEventLog::ValidatorDeposit { depositor, deposit } => {
                // okay to ignore, it is initial sync
                if let Some(pending) = self.pending.front_mut() {
                    // TODO check casting between i64 and u64
                    *pending.stake_diff.entry(depositor).or_insert(0) -= deposit as i64;
                }
            }
            EthEventLog::ValidatorWithdrawal {
                withdrawer,
                withdrawal,
            } => {
                // okay to ignore, it is initial sync
                if let Some(pending) = self.pending.front_mut() {
                    *pending.stake_diff.entry(withdrawer).or_insert(0) += withdrawal as i64;
                }
            }
            EthEventLog::FuelBlockCommited { .. } => {
                //fuel block commit reverted, just pop from pending deque
                if !self.pending.is_empty() {
                    self.pending.pop_front();
                }
                if let Some(parent) = self.pending.front_mut() {
                    parent.eth_number = u64::MAX;
                }
            }
        }
    }

    /// At begining we will ignore all event until event for new fuel block commit commes
    /// after that syncronization can start.
    async fn append_eth_events(&mut self, fuel_event: &EthEventLog, eth_block_number: u64) {
        match *fuel_event {
            EthEventLog::AssetDeposit {
                account,
                token,
                amount,
                deposit_nonce,
                ..
            } => {
                // what to do with deposit_nonce and block_number?
                if let Some(pending) = self.pending.front_mut() {
                    pending
                        .assets_deposited
                        .insert(deposit_nonce, (account, token, amount));
                }
            }
            EthEventLog::ValidatorDeposit { depositor, deposit } => {
                // okay to ignore, it is initial sync
                if let Some(pending) = self.pending.front_mut() {
                    // overflow is not possible
                    *pending.stake_diff.entry(depositor).or_insert(0) += deposit as i64;
                }
            }
            EthEventLog::ValidatorWithdrawal {
                withdrawer,
                withdrawal,
            } => {
                // okay to ignore, it is initial sync
                if let Some(pending) = self.pending.front_mut() {
                    // underflow should not be possible and it should be restrained by contract
                    *pending.stake_diff.entry(withdrawer).or_insert(0) -= withdrawal as i64;
                }
            }
            EthEventLog::FuelBlockCommited { height, .. } => {
                if let Some(parent) = self.pending.front_mut() {
                    parent.eth_number = eth_block_number;
                }
                self.pending.push_front(PendingDiff::new(height));
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
            // not formated event from contract
            warn!(target:"relayer", "Eth Event not formated properly:{}",err);
            return;
        }
        let fuel_event = fuel_event.unwrap();
        // check if this is event from reorg block. if it is we just save it for later processing.
        // and only after all removed logs are received we apply them.
        if let Some(true) = eth_event.removed {
            // agregate all removed events before reverting them.
            if let Some(eth_block) = eth_event.block_number {
                // check if we have pending block for removal
                if let Some((last_eth_block, list)) = self.pending_removed_eth_events.last_mut() {
                    // check if last pending block is same as log event that we received.
                    if *last_eth_block == eth_block.as_u64() {
                        // just push it
                        list.push(fuel_event)
                    } else {
                        // if block number differs just push new block.
                        self.pending_removed_eth_events
                            .push((eth_block.as_u64(), vec![fuel_event]));
                    }
                } else {
                    // if there are not pending block for removal just add it.
                    self.pending_removed_eth_events
                        .push((eth_block.as_u64(), vec![fuel_event]));
                }
            } else {
                error!("Block number not found in eth log");
            }
            return;
        }
        // apply all reverted event
        if !self.pending_removed_eth_events.is_empty() {
            info!(target:"relayer", "Reorg happened on ethereum. Reverting {} logs",self.pending_removed_eth_events.len());

            // if there is new log that is not removed it means we can revert our pending removed eth events.
            for (_, block_events) in
                std::mem::take(&mut self.pending_removed_eth_events).into_iter()
            {
                for fuel_event in block_events.into_iter().rev() {
                    self.revert_eth_event(&fuel_event).await;
                }
            }
        }

        // apply new event to pending queue
        self.append_eth_events(&fuel_event, eth_event.block_number.unwrap().as_u64())
            .await;
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use ethers_core::types::{BlockNumber, FilterBlockOption, U256, U64};
    use ethers_providers::SyncingStatus;
    use fuel_core_interfaces::relayer::RelayerEvent;

    use crate::{
        test::{relayer, FooReturn, MockData, MockMiddleware, TriggerType},
        Config,
    };

    #[tokio::test]
    pub async fn initialsync_aggregate_first_n_events() {
        let mut config = Config::new();
        config.eth_v2_contract_deployment = 5;
        let (relayer, event, _) = relayer(config);
        let middle = MockMiddleware::new();
        middle.data.lock().is_syncing = SyncingStatus::IsSyncing {
            starting_block: U256::zero(),
            current_block: U256::zero(),
            highest_block: U256::zero(),
        };

        let mut i = 0;
        middle.triggers.lock().push(Box::new(
            move |_: &mut MockData, trigger: TriggerType| -> FooReturn {
                Box::pin(async move {
                    if matches!(trigger, TriggerType::Syncing) {
                        i += 1;
                        if i == 2 {
                            assert!(false, "Stop signal not handled");
                        }
                    }
                    false
                })
            },
        ));

        let join = tokio::spawn(relayer.run(middle, 10));
        tokio::time::sleep(Duration::from_millis(10)).await;
        let _ = event.send(RelayerEvent::Stop).await;
        let _ = join.await;
    }

    #[tokio::test]
    pub async fn sync_first_5_block() {
        let mut config = Config::new();
        // start from block 1
        config.eth_v2_contract_deployment = 100;
        config.eth_finality_slider = 30;
        // make 2 steps of 2 blocks
        config.initial_sync_step = 2;
        let (relayer, event, _) = relayer(config);
        let middle = MockMiddleware::new();
        {
            let mut data = middle.data.lock();
            // eth finished syncing
            data.is_syncing = SyncingStatus::IsFalse;
            // best block is 4
            data.best_block.number = Some(U64([134]));
        }
        let mut i = 0;
        middle.triggers.lock().push(Box::new(
            move |_: &mut MockData, trigger: TriggerType| -> FooReturn {
                let event = event.clone();
                Box::pin(async move {
                    println!("trigger:{:?}", trigger);
                    if i == 2 {
                        println!("SENDD STOPPP");
                        let event = event.clone();
                        let _ = tokio::spawn(async move { event.send(RelayerEvent::Stop).await });
                        return false;
                    }
                    if let TriggerType::GetLogs(filter) = trigger {
                        if let FilterBlockOption::Range {
                            from_block,
                            to_block,
                        } = filter.block_option
                        {
                            assert_eq!(
                                from_block,
                                Some(BlockNumber::Number(U64([100 + i * 2]))),
                                "Start block not matching on i:{:?}",
                                i
                            );
                            assert_eq!(
                                to_block,
                                Some(BlockNumber::Number(U64([102 + i * 2]))),
                                "Start block not matching on i:{:?}",
                                i
                            );
                            i += 1;
                        }
                    }
                    false
                })
            },
        ));

        let join = tokio::spawn(relayer.run(middle, 10));
        //tokio::time::sleep(Duration::from_millis(10)).await;
        let _ = join.await;
        println!("the END")
    }
}
