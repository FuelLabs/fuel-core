use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use ethers_core::types::H160;
use ethers_providers::Middleware;
use fuel_core_interfaces::{
    model::{BlockHeight, SealedFuelBlock},
    relayer::{RelayerDb, StakingDiff},
};
use fuel_tx::{Address, Bytes32};
use tracing::{error, info};

use crate::{
    block_commit::BlockCommit,
    log::{AssetDepositLog, EthEventLog},
};

pub struct PendingEvents {
    /// Pending stakes/assets/withdrawals. Before they are finalized
    /// it contains every fuel block and its span
    pending: VecDeque<PendingDiff>,
    /// This is little bit hacky but because we relate validator staking with fuel commit block and not on eth block
    /// we need to be sure that we are taking proper order of those transactions
    /// Revert are reported as list of reverted logs in order of Block2Log1,Block2Log2,Block1Log1,Block2Log2.
    /// I checked this with infura endpoint.
    bundled_removed_eth_events: Vec<(u64, Vec<EthEventLog>)>,
    /// finalized fuel block
    finalized_da_height: u64,
    /// Pending block handling
    block_commit: BlockCommit,
}

/// Pending diff between FuelBlocks
#[derive(Clone, Debug, Default)]
pub struct PendingDiff {
    /// eth block number, It represent height until when we are taking stakes and token deposits.
    /// It is always monotonic and check on its limits are checked in consensus and in contract.
    /// Contract needs to check that when feul block is commited that this number is more then
    /// finality period N.
    pub da_height: u64,
    /// Validator stake deposit and withdrawel.
    pub validators: HashMap<Address, Option<Address>>,
    // Delegation diff contains new delegation list, if we did just withdrawal option will be None.
    pub delegations: HashMap<Address, Option<HashMap<Address, u64>>>,
    /// erc-20 pending deposit.
    pub assets: HashMap<Bytes32, AssetDepositLog>,
}

impl PendingDiff {
    pub fn new(da_height: u64) -> Self {
        Self {
            da_height,
            validators: HashMap::new(),
            delegations: HashMap::new(),
            assets: HashMap::new(),
        }
    }
}

impl PendingEvents {
    pub fn new(
        chain_id: u64,
        contract_address: H160,
        private_key: Vec<u8>,
        last_commited_finalized_fuel_height: BlockHeight,
    ) -> Self {
        let block_commit = BlockCommit::new(
            chain_id,
            contract_address,
            private_key,
            last_commited_finalized_fuel_height,
        );
        Self {
            block_commit,
            pending: VecDeque::new(),
            bundled_removed_eth_events: Vec::new(),
            finalized_da_height: 0,
        }
    }

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

    /// propagate new fuel block to block_commit
    pub fn handle_fuel_block(&mut self, block: &Arc<SealedFuelBlock>) {
        self.block_commit.set_chain_height(block.header.height)
    }

    /// propagate new created fuel block to block_commit
    pub async fn handle_created_fuel_block<P>(
        &mut self,
        block: &Arc<SealedFuelBlock>,
        db: &mut dyn RelayerDb,
        provider: &Arc<P>,
    ) where
        P: Middleware + 'static,
    {
        self.block_commit
            .created_block(block.clone(), db, provider)
            .await;
    }

    /// Handle eth log events
    pub async fn handle_eth_log(&mut self, event: EthEventLog, eth_block: u64, removed: bool) {
        info!("handle eth log:{:?}", event);
        // bundle removed events and return
        if removed {
            self.bundle_removed_events(event, eth_block);
            return;
        }
        // apply all reverted event
        if !self.bundled_removed_eth_events.is_empty() {
            info!(
                "Reorg happened on ethereum. Reverting {} logs",
                self.bundled_removed_eth_events.len()
            );

            let mut lowest_removed_da_height = u64::MAX;

            for (da_height, events) in
                std::mem::take(&mut self.bundled_removed_eth_events).into_iter()
            {
                lowest_removed_da_height = u64::min(lowest_removed_da_height, da_height);
                // mark all removed pending block commits as reverted.
                for event in events {
                    if let EthEventLog::FuelBlockCommited { block_root, height } = event {
                        self.block_commit.block_commit_reverted(
                            block_root,
                            height.into(),
                            da_height.into(),
                        );
                    }
                }
            }
            // remove all blocks that were reverted. In best case those blocks heights and events are going
            // to be reinserted in append eth events.
            self.pending
                .retain(|diff| diff.da_height < lowest_removed_da_height);
        }
        // apply new event to pending queue
        self.append_eth_events(event, eth_block).await;
    }

    /// At begining we will ignore all event until event for new fuel block commit commes
    /// after that syncronization can start.
    pub async fn append_eth_events(&mut self, fuel_event: EthEventLog, da_height: u64) {
        if let Some(front) = self.pending.back() {
            if front.da_height != da_height {
                self.pending.push_back(PendingDiff::new(da_height))
            }
        } else {
            self.pending.push_back(PendingDiff::new(da_height))
        }
        let last_diff = self.pending.back_mut().unwrap();
        match fuel_event {
            EthEventLog::AssetDeposit(deposit) => {
                last_diff.assets.insert(deposit.deposit_nonce, deposit);
            }
            EthEventLog::Deposit { .. } => {
                // It is fine to do nothing. This is only related to contract,
                // only possible usage for this is as additional information for user.
            }
            EthEventLog::Withdrawal { withdrawer, .. } => {
                last_diff.delegations.insert(withdrawer, None);
            }
            EthEventLog::Delegation {
                delegator,
                delegates,
                amounts,
            } => {
                let delegates: HashMap<_, _> = delegates
                    .iter()
                    .zip(amounts.iter())
                    .map(|(f, s)| (*f, *s))
                    .collect();
                last_diff.delegations.insert(delegator, Some(delegates));
            }
            EthEventLog::ValidatorRegistration {
                staking_key,
                consensus_key,
            } => {
                last_diff
                    .validators
                    .insert(staking_key, Some(consensus_key));
            }
            EthEventLog::ValidatorUnregistration { staking_key } => {
                last_diff.validators.insert(staking_key, None);
            }
            EthEventLog::FuelBlockCommited { height, block_root } => {
                self.block_commit
                    .block_commited(block_root, (height).into(), da_height.into());
            }
        }
    }

    /// Used in two places. On initial sync and when new fuel blocks is
    pub async fn commit_diffs(&mut self, db: &mut dyn RelayerDb, finalized_da_height: u64) {
        if self.finalized_da_height >= finalized_da_height {
            error!(
                "We received finalized height {} but we already have {}",
                finalized_da_height, self.finalized_da_height
            );
            return;
        }
        while let Some(diff) = self.pending.front_mut() {
            if diff.da_height > finalized_da_height {
                break;
            }
            info!("flush eth log:{:?} diff:{:?}", diff.da_height, diff);
            //TODO to be paranoid, recheck events got from eth client.

            // apply staking diffs
            db.insert_staking_diff(
                diff.da_height,
                &StakingDiff::new(diff.validators.clone(), diff.delegations.clone()),
            )
            .await;

            // append index of delegator so that we cross reference earliest delegation set
            for (delegate, _) in diff.delegations.iter() {
                db.append_delegate_index(delegate, diff.da_height).await;
            }

            // push finalized assets to db
            for (_, deposit) in diff.assets.iter() {
                db.insert_coin_deposit(deposit.into()).await
            }

            // insert height index into delegations.
            db.set_finalized_da_height(diff.da_height).await;

            // remove pending diff
            self.pending.pop_front();
        }

        self.block_commit.new_da_block(finalized_da_height.into());

        db.set_last_commited_finalized_fuel_height(
            self.block_commit.last_commited_finalized_fuel_height(),
        )
        .await;
        self.finalized_da_height = finalized_da_height;
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::log::tests::*;
    use fuel_types::{Address, AssetId};

    #[tokio::test]
    pub async fn check_token_deposits_on_multiple_eth_blocks() {
        let acc1 = Address::from([1; 32]);
        let token1 = AssetId::zeroed();
        let nonce1 = Bytes32::from([2; 32]);
        let nonce2 = Bytes32::from([3; 32]);
        let nonce3 = Bytes32::from([4; 32]);

        let mut pending = PendingEvents::new(
            0,
            H160::zero(),
            hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap(),
            BlockHeight::from(0u64),
        );

        let deposit1 = eth_log_asset_deposit(0, acc1, token1, 0, 10, nonce1, 0);
        let deposit2 = eth_log_asset_deposit(1, acc1, token1, 0, 20, nonce2, 0);
        let deposit3 = eth_log_asset_deposit(1, acc1, token1, 0, 40, nonce3, 0);

        let deposit1 = EthEventLog::try_from(&deposit1).unwrap();
        let deposit2 = EthEventLog::try_from(&deposit2).unwrap();
        let deposit3 = EthEventLog::try_from(&deposit3).unwrap();

        pending.handle_eth_log(deposit1.clone(), 0, false).await;
        pending.handle_eth_log(deposit2.clone(), 1, false).await;
        pending.handle_eth_log(deposit3.clone(), 1, false).await;
        let diff1 = pending.pending[0].clone();
        let diff2 = pending.pending[1].clone();

        if let EthEventLog::AssetDeposit(deposit) = &deposit1 {
            assert_eq!(
                diff1.assets.get(&nonce1),
                Some(deposit),
                "Deposit 1 not valid"
            );
        }
        if let EthEventLog::AssetDeposit(deposit) = &deposit2 {
            assert_eq!(
                diff2.assets.get(&nonce2),
                Some(deposit),
                "Deposit 2 not valid"
            );
        }
        if let EthEventLog::AssetDeposit(deposit) = &deposit3 {
            assert_eq!(
                diff2.assets.get(&nonce3),
                Some(deposit),
                "Deposit 3 not valid"
            );
        }
    }

    /*
    #[tokio::test]
    pub async fn check_validator_set_deposits_on_multiple_eth_blocks() {
        let acc1 = Address::from([1; 32]);
        let acc2 = Address::from([2; 32]);

        let mut pending = PendingEvents::new(
            0,
            H160::zero(),
            hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap(),
        );

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

        let mut pending = PendingEvents::new(
            0,
            H160::zero(),
            hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap(),
        );

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
    } */
}
