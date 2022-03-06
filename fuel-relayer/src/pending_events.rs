use std::collections::{HashMap, VecDeque};

use fuel_core_interfaces::relayer::RelayerDB;
use fuel_tx::{Address, Bytes32, Color};
use fuel_types::Word;
use tracing::info;

use crate::log::EthEventLog;

pub struct PendingEvents {
    /// Pendning stakes/assets/withdrawals. Before they are finalized
    /// it contains every fuel block and its span
    pending: VecDeque<PendingDiff>,
    /// This is litlle bit hacky but because we relate validator staking with fuel commit block and not on eth block
    /// we need to be sure that we are taking proper order of those transactions
    /// Revert are reported as list of reverted logs in order of Block2Log1,Block2Log2,Block1Log1,Block2Log2.
    /// I checked this with infura endpoint.
    bundled_removed_eth_events: Vec<(u64, Vec<EthEventLog>)>,
    /// finalized validator set
    finalized_validator_set: HashMap<Address, u64>,
    /// finalized fuel block
    finalized_eth_height: u64,
}

// When we are adding new block we need to specify eth block in past that will include all token deposits and
// new validator set changes. Rules that contract need to enforce is.
// 1. NewBlock eth_block need to be same or more then current last_consumed_block.
// 2. In batched fuel block commit this is still enforced for every block inside. This will be litlle bit strained on contract
// as it need to enforce  validator set change.
// 3. In batched fuel block it is

/// Pending diff between FuelBlocks
#[derive(Clone, Debug)]
pub struct PendingDiff {
    /// eth block number, It represent until when we are taking stakes and token deposits.
    /// It is always monotonic and check on its limits are check in consensus and in contract.
    /// Contract needs to check that when feul block is commited that this number is more then
    /// finality period N.
    eth_height: u64,
    /// Validator stake deposit and withdrawel.
    stake_diff: HashMap<Address, i64>,
    /// erc-20 pending deposit. deposit nonce.
    assets_deposited: HashMap<Bytes32, (Address, Color, Word)>,
}

impl PendingDiff {
    pub fn new(eth_height: u64) -> Self {
        Self {
            eth_height,
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

impl PendingEvents {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            bundled_removed_eth_events: Vec::new(),
            finalized_validator_set: HashMap::new(),
            finalized_eth_height: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn pop_front(&mut self) -> Option<PendingDiff> {
        self.pending.pop_front()
    }

    pub fn clear(&mut self) {
        self.pending.clear()
    }

    pub fn push_front(&mut self, pending: PendingDiff) {
        self.pending.push_front(pending)
    }

    /// Bundle all removed events to apply them in same time when all of them are flushed.
    /// Done
    fn bundle_removed_events(&mut self, event: EthEventLog, eth_block: u64) {
        // agregate all removed events before reverting them.
        // check if we have pending block for removal
        if let Some((last_eth_block, list)) = self.bundled_removed_eth_events.last_mut() {
            // check if last pending block is same as log event that we received.
            if *last_eth_block == eth_block {
                // just push it
                list.push(event)
            } else {
                // if block number differs just push new block.
                self.bundled_removed_eth_events
                    .push((eth_block, vec![event]));
            }
        } else {
            // if there are not pending block for removal just add it.
            self.bundled_removed_eth_events
                .push((eth_block, vec![event]));
        }
    }

    /// Handle eth log events
    /// Done
    pub async fn handle_eth_event(&mut self, event: EthEventLog, eth_block: u64, removed: bool) {
        if removed {
            self.bundle_removed_events(event, eth_block);
            return;
        }
        // apply all reverted event
        if !self.bundled_removed_eth_events.is_empty() {
            info!(target:"relayer", "Reorg happened on ethereum. Reverting {} logs",self.bundled_removed_eth_events.len());

            // if there is new log that is not removed it means we can revert our pending removed eth events.
            let mut current_eth_height = 0;
            for (eth_height, _) in std::mem::take(&mut self.bundled_removed_eth_events).into_iter()
            {
                if eth_height != current_eth_height {
                    self.pending.pop_front();
                    current_eth_height = eth_height;
                }
            }
        }
        // apply new event to pending queue
        self.append_eth_events(&event, eth_block).await;
    }

    /// At begining we will ignore all event until event for new fuel block commit commes
    /// after that syncronization can start.
    /// Done
    pub async fn append_eth_events(&mut self, fuel_event: &EthEventLog, eth_height: u64) {
        if let Some(front) = self.pending.front() {
            if front.eth_height != eth_height {
                self.pending.push_front(PendingDiff::new(eth_height))
            }
        }
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
            EthEventLog::FuelBlockCommited { .. } => {
                // TODO do nothing? or maybe update some state as BlockCommitSeen, BlockCommitFinalized, etc.
            }
        }
    }

    /// Used in two places. On initial sync and when new fuel blocks is
    /// Done
    pub async fn apply_last_validator_diff(
        &mut self,
        db: &mut dyn RelayerDB,
        finalized_eth_block: u64,
    ) {
        while let Some(diffs) = self.pending.back() {
            if diffs.eth_height <= finalized_eth_block {
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
            db.insert_validator_set_diff(diffs.eth_height, &stake_diff)
                .await;
            db.set_fuel_finalized_block(diffs.eth_height).await;

            // push fanalized deposit to db
            for (nonce, deposit) in diffs.assets_deposited.iter() {
                db.insert_token_deposit(*nonce, diffs.eth_height, deposit.0, deposit.1, deposit.2)
                    .await
            }
            self.pending.pop_back();
        }
        self.finalized_eth_height = finalized_eth_block;
        db.set_eth_finalized_block(finalized_eth_block).await;
    }
}
