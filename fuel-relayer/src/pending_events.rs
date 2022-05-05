use std::collections::{HashMap, VecDeque};

use fuel_core_interfaces::relayer::RelayerDb;
use fuel_tx::{Address, AssetId, Bytes32};
use fuel_types::Word;
use tracing::info;

use crate::log::EthEventLog;

#[derive(Default)]
pub struct PendingEvents {
    /// Pending stakes/assets/withdrawals. Before they are finalized
    /// it contains every fuel block and its span
    pending: VecDeque<PendingDiff>,
    /// This is little bit hacky but because we relate validator staking with fuel commit block and not on eth block
    /// we need to be sure that we are taking proper order of those transactions
    /// Revert are reported as list of reverted logs in order of Block2Log1,Block2Log2,Block1Log1,Block2Log2.
    /// I checked this with infura endpoint.
    bundled_removed_eth_events: Vec<(u64, Vec<EthEventLog>)>,
    /// finalized validator set
    finalized_validator_set: HashMap<Address, u64>,
    /// finalized fuel block
    finalized_da_height: u64,
}

/// Pending diff between FuelBlocks
#[derive(Clone, Debug, Default)]
pub struct PendingDiff {
    /// eth block number, It represent until when we are taking stakes and token deposits.
    /// It is always monotonic and check on its limits are check in consensus and in contract.
    /// Contract needs to check that when feul block is commited that this number is more then
    /// finality period N.
    da_height: u64,
    /// Validator stake deposit and withdrawel.
    stake_diff: HashMap<Address, i64>,
    /// erc-20 pending deposit. deposit nonce.
    assets_deposited: HashMap<Bytes32, (Address, AssetId, Word)>,
}

impl PendingDiff {
    pub fn new(da_height: u64) -> Self {
        Self {
            da_height,
            stake_diff: HashMap::new(),
            assets_deposited: HashMap::new(),
        }
    }
    pub fn stake_diff(&self) -> &HashMap<Address, i64> {
        &self.stake_diff
    }
    pub fn assets_deposited(&self) -> &HashMap<Bytes32, (Address, AssetId, Word)> {
        &self.assets_deposited
    }
}

impl PendingEvents {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn pop_back(&mut self) -> Option<PendingDiff> {
        self.pending.pop_back()
    }

    pub fn clear(&mut self) {
        self.pending.clear()
    }

    pub fn push_back(&mut self, pending: PendingDiff) {
        self.pending.push_back(pending)
    }

    /// Bundle all removed events to apply them in same time when all of them are flushed.
    fn bundle_removed_events(&mut self, event: EthEventLog, eth_block: u64) {
        // agregate all removed events before reverting them.
        // check if we have pending block for removal
        if let Some((last_eth_block, list)) = self.bundled_removed_eth_events.last_mut() {
            // check if last pending block is same as log event that we received.
            if *last_eth_block == eth_block {
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
    pub async fn handle_eth_log(&mut self, event: EthEventLog, eth_block: u64, removed: bool) {
        if removed {
            self.bundle_removed_events(event, eth_block);
            return;
        }
        // apply all reverted event
        if !self.bundled_removed_eth_events.is_empty() {
            info!(target:"relayer", "Reorg happened on ethereum. Reverting {} logs",self.bundled_removed_eth_events.len());

            // if there is new log that is not removed it means we can revert our pending removed eth events.
            let mut current_da_height = 0;
            for (da_height, _) in std::mem::take(&mut self.bundled_removed_eth_events).into_iter() {
                if da_height != current_da_height {
                    self.pending.pop_back();
                    current_da_height = da_height;
                }
            }
        }
        // apply new event to pending queue
        self.append_eth_events(&event, eth_block).await;
    }

    /// At begining we will ignore all event until event for new fuel block commit commes
    /// after that syncronization can start.
    pub async fn append_eth_events(&mut self, fuel_event: &EthEventLog, da_height: u64) {
        if let Some(front) = self.pending.back() {
            if front.da_height != da_height {
                self.pending.push_back(PendingDiff::new(da_height))
            }
        } else {
            self.pending.push_back(PendingDiff::new(da_height))
        }
        let last_diff = self.pending.back_mut().unwrap();
        match *fuel_event {
            EthEventLog::AssetDeposit {
                account,
                token,
                amount,
                deposit_nonce,
                ..
            } => {
                last_diff
                    .assets_deposited
                    .insert(deposit_nonce, (account, token, amount));
            }
            EthEventLog::ValidatorDeposit { depositor, deposit } => {
                // overflow is not possible
                *last_diff.stake_diff.entry(depositor).or_insert(0) += deposit as i64;
            }
            EthEventLog::ValidatorWithdrawal {
                withdrawer,
                withdrawal,
            } => {
                // underflow should not be possible and it should be restrained by contract
                *last_diff.stake_diff.entry(withdrawer).or_insert(0) -= withdrawal as i64;
            }
            EthEventLog::FuelBlockCommited { .. } => {
                // TODO do nothing? or maybe update some state as BlockCommitSeen, BlockCommitFinalized, etc.
            }
        }
    }

    /// Used in two places. On initial sync and when new fuel blocks is
    pub async fn commit_diffs(&mut self, db: &mut dyn RelayerDb, finalized_da_height: u64) {
        while let Some(diffs) = self.pending.front() {
            if diffs.da_height > finalized_da_height {
                break;
            }
            //TODO to be paranoid, recheck events got from eth client.
            let mut stake_diff = HashMap::new();
            // apply diff to validator_set
            for (address, diff) in &diffs.stake_diff {
                let value = self.finalized_validator_set.entry(*address).or_insert(0);
                // we are okay to cast it, we dont expect that big of number to exist.
                *value = ((*value as i64) + diff) as u64;
                stake_diff.insert(*address, *value);
            }
            // push new value for changed validators to database
            db.insert_validators_diff(diffs.da_height, &stake_diff)
                .await;
            db.set_finalized_da_height(diffs.da_height).await;

            // push finalized deposit to db
            for (nonce, deposit) in diffs.assets_deposited.iter() {
                db.insert_token_deposit(*nonce, diffs.da_height, deposit.0, deposit.1, deposit.2)
                    .await
            }
            self.pending.pop_front();
        }
        self.finalized_da_height = finalized_da_height;
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::log::tests::*;
    use fuel_core_interfaces::db::helpers::DummyDb;
    use fuel_types::Address;

    #[tokio::test]
    pub async fn check_token_deposits_on_multiple_eth_blocks() {
        let acc1 = Address::from([1; 32]);
        let token1 = AssetId::zeroed();
        let nonce1 = Bytes32::from([2; 32]);
        let nonce2 = Bytes32::from([3; 32]);
        let nonce3 = Bytes32::from([4; 32]);

        let mut pending = PendingEvents::default();

        let deposit1 =
            EthEventLog::try_from(&eth_log_asset_deposit(0, acc1, token1, 0, 10, nonce1)).unwrap();
        let deposit2 =
            EthEventLog::try_from(&eth_log_asset_deposit(1, acc1, token1, 0, 20, nonce2)).unwrap();
        let deposit3 =
            EthEventLog::try_from(&eth_log_asset_deposit(1, acc1, token1, 0, 40, nonce3)).unwrap();

        pending.handle_eth_log(deposit1, 0, false).await;
        pending.handle_eth_log(deposit2, 1, false).await;
        pending.handle_eth_log(deposit3, 1, false).await;
        let diff1 = pending.pending[0].clone();
        let diff2 = pending.pending[1].clone();
        assert_eq!(
            diff1.assets_deposited.get(&nonce1),
            Some(&(acc1, token1, 10)),
            "Deposit 1 not valid"
        );
        assert_eq!(
            diff2.assets_deposited.get(&nonce2),
            Some(&(acc1, token1, 20)),
            "Deposit 2 not valid"
        );
        assert_eq!(
            diff2.assets_deposited.get(&nonce3),
            Some(&(acc1, token1, 40)),
            "Deposit 3 not valid"
        );
    }

    #[tokio::test]
    pub async fn check_validator_set_deposits_on_multiple_eth_blocks() {
        let acc1 = Address::from([1; 32]);
        let acc2 = Address::from([2; 32]);

        let mut pending = PendingEvents::default();

        let deposit1 = EthEventLog::try_from(&eth_log_validator_deposit(0, acc1, 1)).unwrap();
        let deposit2 = EthEventLog::try_from(&eth_log_validator_deposit(1, acc1, 20)).unwrap();
        let deposit3 = EthEventLog::try_from(&eth_log_validator_deposit(1, acc2, 5)).unwrap();
        let deposit4 = EthEventLog::try_from(&eth_log_validator_deposit(1, acc2, 60)).unwrap();
        let deposit5 = EthEventLog::try_from(&eth_log_validator_deposit(1, acc1, 300)).unwrap();

        pending.handle_eth_log(deposit1, 0, false).await;
        pending.handle_eth_log(deposit2, 1, false).await;
        pending.handle_eth_log(deposit3, 1, false).await;
        pending.handle_eth_log(deposit4, 1, false).await;
        pending.handle_eth_log(deposit5, 1, false).await;
        let diff1 = pending.pending[0].clone();
        let diff2 = pending.pending[1].clone();
        assert_eq!(
            diff1.stake_diff.get(&acc1),
            Some(&1),
            "Account1 expect 1 diff stake"
        );
        assert_eq!(
            diff1.stake_diff.get(&acc2),
            None,
            "Account2 stake diff should not exist on eth height 0"
        );

        assert_eq!(
            diff2.stake_diff.get(&acc1),
            Some(&320),
            "Account1 expect 320 diff stake"
        );
        assert_eq!(
            diff2.stake_diff.get(&acc2),
            Some(&65),
            "Account2 expect 65 diff stake"
        );
    }

    #[tokio::test]
    pub async fn check_validator_set_withdrawal_on_multiple_eth_blocks() {
        let acc1 = Address::from([1; 32]);
        let acc2 = Address::from([2; 32]);

        let mut pending = PendingEvents::default();

        let deposit1 = EthEventLog::try_from(&eth_log_validator_deposit(0, acc1, 1000)).unwrap();
        let deposit2 = EthEventLog::try_from(&eth_log_validator_withdrawal(1, acc1, 300)).unwrap();
        let deposit3 = EthEventLog::try_from(&eth_log_validator_deposit(1, acc2, 60)).unwrap();
        let deposit4 = EthEventLog::try_from(&eth_log_validator_withdrawal(1, acc2, 10)).unwrap();
        let deposit5 = EthEventLog::try_from(&eth_log_validator_withdrawal(1, acc1, 100)).unwrap();
        let deposit6 = EthEventLog::try_from(&eth_log_validator_withdrawal(2, acc1, 50)).unwrap();

        pending.handle_eth_log(deposit1, 0, false).await;
        pending.handle_eth_log(deposit2, 1, false).await;
        pending.handle_eth_log(deposit3, 1, false).await;
        pending.handle_eth_log(deposit4, 1, false).await;
        pending.handle_eth_log(deposit5, 1, false).await;
        pending.handle_eth_log(deposit6, 2, false).await;
        let diff1 = pending.pending[0].clone();
        let diff2 = pending.pending[1].clone();
        let diff3 = pending.pending[2].clone();
        assert_eq!(
            diff1.stake_diff.get(&acc1),
            Some(&1000),
            "Account1 expect 1000 diff stake"
        );
        assert_eq!(
            diff2.stake_diff.get(&acc1),
            Some(&-400),
            "Account1 expect -500 diff stake"
        );
        assert_eq!(
            diff2.stake_diff.get(&acc2),
            Some(&50),
            "Account2 expect 60-10 diff stake"
        );

        assert_eq!(
            diff3.stake_diff.get(&acc1),
            Some(&-50),
            "Account1 expect -50 diff stake"
        );

        // apply all diffs to finalized state
        let mut db = DummyDb::filled();

        pending.commit_diffs(&mut db, 5).await;

        assert_eq!(pending.pending.len(), 0, "All diffs should be flushed");
        assert_eq!(pending.finalized_da_height, 5, "Finalized should be 5");
        assert_eq!(
            pending.finalized_validator_set.get(&acc1),
            Some(&550),
            "Acc1 state should be "
        );
        assert_eq!(
            pending.finalized_validator_set.get(&acc2),
            Some(&50),
            "Acc2 state should be "
        );

        let data = db.data.lock();
        let diffs = &data.validator_set_diff;
        assert_eq!(diffs.len(), 3);
        assert_eq!(diffs.get(&0).unwrap().get(&acc1), Some(&1000));
        assert_eq!(diffs.get(&1).unwrap().get(&acc1), Some(&600));
        assert_eq!(diffs.get(&1).unwrap().get(&acc2), Some(&50));
        assert_eq!(diffs.get(&2).unwrap().get(&acc1), Some(&550));
    }
}
