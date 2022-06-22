use crate::{
    log::{AssetDepositLog, EthEventLog},
    pending_blocks::{IsReverted, PendingBlocks},
    validators::Validators,
};
use ethers_core::types::{Log, H160};
use ethers_providers::Middleware;
use fuel_core_interfaces::{
    common::fuel_tx::{Address, Bytes32},
    model::{
        BlockHeight, ConsensusId, DaBlockHeight, SealedFuelBlock, ValidatorId, ValidatorStake,
    },
    relayer::{RelayerDb, StakingDiff, ValidatorDiff},
};
use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    sync::Arc,
};
use tracing::{debug, error, info, warn};

pub struct FinalizationQueue {
    /// Pending stakes/assets/withdrawals. Before they are finalized
    pending: VecDeque<DaBlockDiff>,
    /// Revert on eth are reported as list of reverted logs in order of Block2Log1,Block2Log2,Block1Log1,Block2Log2.
    /// So when applying multiple block reverts it is good to mind the order.
    bundled_removed_eth_events: Vec<(DaBlockHeight, Vec<EthEventLog>)>,
    /// finalized fuel block
    finalized_da_height: DaBlockHeight,
    /// Pending block handling
    blocks: PendingBlocks,
    /// Current validator set
    validators: Validators,
}

/// Pending diff between FuelBlocks
#[derive(Clone, Debug, Default)]
pub struct DaBlockDiff {
    /// da block height
    pub da_height: DaBlockHeight,
    /// Validator stake deposit and withdrawel.
    pub validators: HashMap<ValidatorId, Option<ConsensusId>>,
    // Delegation diff contains new delegation list, if we did just withdrawal option will be None.
    pub delegations: HashMap<Address, Option<HashMap<ValidatorId, ValidatorStake>>>,
    /// erc-20 pending deposit.
    pub assets: HashMap<Bytes32, AssetDepositLog>,
}

impl DaBlockDiff {
    pub fn new(da_height: DaBlockHeight) -> Self {
        Self {
            da_height,
            validators: HashMap::new(),
            delegations: HashMap::new(),
            assets: HashMap::new(),
        }
    }
}

impl FinalizationQueue {
    pub fn new(
        chain_id: u64,
        contract_address: H160,
        private_key: &[u8],
        chain_height: BlockHeight,
        last_commited_finalized_fuel_height: BlockHeight,
    ) -> Self {
        let blocks = PendingBlocks::new(
            chain_id,
            contract_address,
            private_key,
            chain_height,
            last_commited_finalized_fuel_height,
        );
        Self {
            blocks,
            pending: VecDeque::new(),
            validators: Validators::default(),
            bundled_removed_eth_events: Vec::new(),
            finalized_da_height: 0,
        }
    }

    pub async fn load_validators(&mut self, db: &dyn RelayerDb) {
        self.validators.load(db).await
    }

    pub async fn get_validators(
        &mut self,
        da_height: DaBlockHeight,
        db: &mut dyn RelayerDb,
    ) -> Option<HashMap<ValidatorId, (u64, Option<ConsensusId>)>> {
        self.validators.get(da_height, db).await
    }

    pub fn clear(&mut self) {
        self.pending.clear()
    }

    /// Bundle all removed events to apply them in same time when all of them are flushed.
    fn bundle_removed_events(&mut self, event: EthEventLog, da_height: DaBlockHeight) {
        // agregate all removed events before reverting them.
        // check if we have pending block for removal
        if let Some((last_eth_block, list)) = self.bundled_removed_eth_events.last_mut() {
            // check if last pending block is same as log event that we received.
            if *last_eth_block == da_height {
                list.push(event)
            } else {
                // if block number differs just push new block.
                self.bundled_removed_eth_events
                    .push((da_height, vec![event]));
            }
        } else {
            // if there are not pending block for removal just add it.
            self.bundled_removed_eth_events
                .push((da_height, vec![event]));
        }
    }

    /// propagate new fuel block to pending_blocks
    pub fn handle_fuel_block(&mut self, block: &SealedFuelBlock) {
        self.blocks.set_chain_height(block.header.height)
    }

    /// propagate new created fuel block to pending_blocks
    pub async fn handle_created_fuel_block<P>(
        &mut self,
        block: &Arc<SealedFuelBlock>,
        db: &mut dyn RelayerDb,
        provider: &Arc<P>,
    ) where
        P: Middleware + 'static,
    {
        self.blocks.commit(block.header.height, db, provider).await;
    }

    pub async fn append_eth_logs(&mut self, logs: Vec<Log>) {
        for log in logs {
            self.append_eth_log(log).await;
        }
    }

    pub fn remove_bundled_reverted_events(&mut self) {
        // apply all reverted event
        if !self.bundled_removed_eth_events.is_empty() {
            info!(
                "Reorg happened on ethereum. Reverting {} logs",
                self.bundled_removed_eth_events.len()
            );

            let mut lowest_removed_da_height = DaBlockHeight::MAX;

            for (da_height, events) in
                std::mem::take(&mut self.bundled_removed_eth_events).into_iter()
            {
                lowest_removed_da_height = DaBlockHeight::min(lowest_removed_da_height, da_height);
                // mark all removed pending block commits as reverted.
                for event in events {
                    if let EthEventLog::FuelBlockCommited { block_root, height } = event {
                        self.blocks.handle_block_commit(
                            block_root,
                            height.into(),
                            da_height,
                            IsReverted::True,
                        );
                    }
                }
            }
            // remove all blocks that were reverted. In best case those blocks heights and events are going
            // to be reinserted in append eth events.
            self.pending
                .retain(|diff| diff.da_height < lowest_removed_da_height);
        }
    }

    /// Handle eth log events
    pub async fn append_eth_log(&mut self, log: Log) {
        let event = EthEventLog::try_from(&log);
        if let Err(err) = event {
            warn!(target:"relayer", "Eth Event not formated properly:{}",err);
            return;
        }
        if log.block_number.is_none() {
            error!(target:"relayer", "Block number not found in eth log");
            return;
        }
        let removed = log.removed.unwrap_or(false);
        let da_height = log.block_number.unwrap().as_u64() as DaBlockHeight;
        let event = event.unwrap();
        debug!("append inbound log:{:?}", event);
        // bundle removed events and return
        if removed {
            self.bundle_removed_events(event, da_height);
            return;
        }
        self.remove_bundled_reverted_events();
        // apply new event to pending queue
        self.append_da_events(event, da_height).await;
    }

    /// Append da events before to finalization queue.
    async fn append_da_events(&mut self, fuel_event: EthEventLog, da_height: DaBlockHeight) {
        if let Some(front) = self.pending.back() {
            if front.da_height != da_height {
                self.pending.push_back(DaBlockDiff::new(da_height))
            }
        } else {
            self.pending.push_back(DaBlockDiff::new(da_height))
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
                self.blocks.handle_block_commit(
                    block_root,
                    (height).into(),
                    da_height,
                    IsReverted::False,
                );
            }
            EthEventLog::Unknown => (),
        }
    }

    /// Used to commit da block diff to database.
    pub async fn commit_diffs(
        &mut self,
        db: &mut dyn RelayerDb,
        finalized_da_height: DaBlockHeight,
    ) {
        if self.finalized_da_height >= finalized_da_height {
            error!(
                "We received finalized height {} but we already have {}",
                finalized_da_height, self.finalized_da_height
            );
            return;
        }
        self.remove_bundled_reverted_events();

        //TODO to be paranoid, recheck every block and all events got from eth client.

        let mut validators: HashMap<ValidatorId, Option<ConsensusId>> = HashMap::new();
        while let Some(diff) = self.pending.front_mut() {
            if diff.da_height > finalized_da_height {
                break;
            }
            info!("flush eth log:{:?} diff:{:?}", diff.da_height, diff);

            let validator_diff: HashMap<ValidatorId, ValidatorDiff> = diff
                .validators
                .iter()
                .map(|(val, &new_consensus_key)| {
                    let previous_consensus_key = match validators.entry(*val) {
                        Entry::Occupied(mut entry) => {
                            core::mem::replace(entry.get_mut(), new_consensus_key)
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(new_consensus_key);
                            self.validators.set.get(val).and_then(|(_, i)| *i)
                        }
                    };
                    (
                        *val,
                        ValidatorDiff {
                            previous_consensus_key,
                            new_consensus_key,
                        },
                    )
                })
                .collect();

            // apply staking diffs
            db.insert_staking_diff(
                diff.da_height,
                &StakingDiff::new(validator_diff, diff.delegations.clone()),
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

        let last_commited_fin_fuel_height = self.blocks.handle_da_finalization(finalized_da_height);

        db.set_last_commited_finalized_fuel_height(last_commited_fin_fuel_height)
            .await;
        self.finalized_da_height = finalized_da_height;
        // bump validator set to last finalized block
        self.validators
            .bump_set_to_da_height(finalized_da_height, db)
            .await
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::log::tests::*;
    use fuel_core_interfaces::{
        common::fuel_types::{Address, AssetId},
        db::helpers::DummyDb,
    };
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    #[tokio::test]
    pub async fn check_token_deposits_on_multiple_eth_blocks() {
        let mut rng = StdRng::seed_from_u64(3020);

        let acc1: Address = rng.gen();
        let token1 = AssetId::zeroed();
        let nonce1: Bytes32 = rng.gen();
        let nonce2: Bytes32 = rng.gen();
        let nonce3: Bytes32 = rng.gen();

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(10u64),
            BlockHeight::from(0u64),
        );

        let deposit1 = eth_log_asset_deposit(0, acc1, token1, 0, 10, nonce1, 0);
        let deposit2 = eth_log_asset_deposit(1, acc1, token1, 1, 20, nonce2, 0);
        let deposit3 = eth_log_asset_deposit(1, acc1, token1, 1, 40, nonce3, 0);

        let deposit1_db = EthEventLog::try_from(&deposit1).unwrap();
        let deposit2_db = EthEventLog::try_from(&deposit2).unwrap();
        let deposit3_db = EthEventLog::try_from(&deposit3).unwrap();

        queue
            .append_eth_logs(vec![deposit1, deposit2, deposit3])
            .await;

        let diff1 = queue.pending[0].clone();
        let diff2 = queue.pending[1].clone();

        if let EthEventLog::AssetDeposit(deposit) = &deposit1_db {
            assert_eq!(diff1.assets.get(&nonce1), Some(deposit),);
        }
        if let EthEventLog::AssetDeposit(deposit) = &deposit2_db {
            assert_eq!(diff2.assets.get(&nonce2), Some(deposit),);
        }
        if let EthEventLog::AssetDeposit(deposit) = &deposit3_db {
            assert_eq!(diff2.assets.get(&nonce3), Some(deposit),);
        }
    }

    #[tokio::test]
    pub async fn check_validator_registration_unregistration() {
        let mut rng = StdRng::seed_from_u64(3020);
        let v1: ValidatorId = rng.gen();
        let v2: ValidatorId = rng.gen();
        let c1: ConsensusId = rng.gen();
        let c2: ConsensusId = rng.gen();

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(10u64),
            BlockHeight::from(0u64),
        );

        queue
            .append_eth_logs(vec![
                eth_log_validator_registration(0, v1, c1),
                eth_log_validator_registration(0, v2, c2),
                eth_log_validator_unregistration(1, v1),
            ])
            .await;

        let diff1 = queue.pending[0].clone();
        let diff2 = queue.pending[1].clone();
        assert_eq!(diff1.validators.get(&v1), Some(&Some(c1)),);
        assert_eq!(diff1.validators.get(&v2), Some(&Some(c2)),);
        assert_eq!(diff2.validators.get(&v1), Some(&None),);
    }

    #[tokio::test]
    pub async fn check_deposit_and_validator_finalization() {
        let mut rng = StdRng::seed_from_u64(3020);
        let v1: ValidatorId = rng.gen();
        let c1: ConsensusId = rng.gen();
        let v2: ValidatorId = rng.gen();
        let c2: ConsensusId = rng.gen();

        let acc1: Address = rng.gen();
        let token1 = AssetId::zeroed();
        let nonce1: Bytes32 = rng.gen();

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(10u64),
            BlockHeight::from(0u64),
        );

        queue
            .append_eth_logs(vec![
                eth_log_validator_registration(1, v1, c1),
                eth_log_validator_registration(2, v2, c2),
                eth_log_asset_deposit(2, acc1, token1, 1, 40, nonce1, 0),
                eth_log_validator_unregistration(3, v1),
            ])
            .await;

        let mut db = DummyDb::filled();
        //let db_ref = &mut db as &mut dyn RelayerDb;

        queue.commit_diffs(&mut db, 1).await;
        assert_eq!(db.data.lock().validators.get(&v1), Some(&(0, Some(c1))),);
        assert_eq!(db.data.lock().validators.get(&v2), None,);
        assert_eq!(db.data.lock().deposit_coin.len(), 0,);

        queue.commit_diffs(&mut db, 2).await;
        assert_eq!(db.data.lock().validators.get(&v2), Some(&(0, Some(c2))),);
        assert_eq!(db.data.lock().deposit_coin.len(), 1,);

        queue.commit_diffs(&mut db, 3).await;
        assert_eq!(db.data.lock().validators.get(&v1), Some(&(0, None)),);
        assert_eq!(db.data.lock().validators.get(&v2), Some(&(0, Some(c2))),);
        assert_eq!(db.data.lock().deposit_coin.len(), 1,);
    }

    #[tokio::test]
    pub async fn delegation_and_withdrawal_finalization() {
        let mut rng = StdRng::seed_from_u64(3020);
        let mut delegator1: Address = rng.gen();
        let mut delegator2: Address = rng.gen();
        delegator1.iter_mut().take(12).for_each(|i| *i = 0);
        delegator2.iter_mut().take(12).for_each(|i| *i = 0);
        let mut v1: ValidatorId = rng.gen();
        let c1: ConsensusId = rng.gen();
        let mut v2: ValidatorId = rng.gen();
        v1.iter_mut().take(12).for_each(|i| *i = 0);
        v2.iter_mut().take(12).for_each(|i| *i = 0);

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(10u64),
            BlockHeight::from(0u64),
        );

        let s1 = rng.gen::<u16>() as u64;
        let s2 = rng.gen::<u16>() as u64;
        let s3 = rng.gen::<u16>() as u64;

        queue
            .append_eth_logs(vec![
                eth_log_delegation(1, delegator1, vec![v1, v2], vec![s1, s2]),
                eth_log_validator_registration(2, v1, c1),
                eth_log_delegation(2, delegator2, vec![v1], vec![s3]),
                eth_log_withdrawal(3, delegator1, 7),
            ])
            .await;
        let mut db = DummyDb::filled();

        queue.commit_diffs(&mut db, 1).await;
        assert_eq!(db.data.lock().validators.get(&v1), Some(&(s1, None)),);
        assert_eq!(db.data.lock().validators.get(&v2), Some(&(s2, None)),);

        queue.commit_diffs(&mut db, 2).await;
        let s13 = s1 + s3;
        assert_eq!(db.data.lock().validators.get(&v1), Some(&(s13, Some(c1))),);

        queue.commit_diffs(&mut db, 3).await;

        assert_eq!(db.data.lock().validators.get(&v1), Some(&(s3, Some(c1))),);
        assert_eq!(db.data.lock().validators.get(&v2), Some(&(0, None)),);
    }

    #[tokio::test]
    pub async fn test_edge_case_of_double_delegation() {
        let mut rng = StdRng::seed_from_u64(3020);
        let mut delegator1: Address = rng.gen();
        let mut delegator2: Address = rng.gen();
        delegator1.iter_mut().take(12).for_each(|i| *i = 0);
        delegator2.iter_mut().take(12).for_each(|i| *i = 0);
        let mut v1: ValidatorId = rng.gen();
        let mut v2: ValidatorId = rng.gen();
        v1.iter_mut().take(12).for_each(|i| *i = 0);
        v2.iter_mut().take(12).for_each(|i| *i = 0);

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(10u64),
            BlockHeight::from(0u64),
        );

        let s1 = rng.gen::<u16>() as u64;
        let s2 = rng.gen::<u16>() as u64;
        let s3 = rng.gen::<u16>() as u64;

        queue
            .append_eth_logs(vec![
                eth_log_delegation(1, delegator1, vec![v1, v2], vec![s1, s2]),
                eth_log_delegation(1, delegator1, vec![v1], vec![s1]),
                eth_log_delegation(1, delegator2, vec![v2], vec![s1]),
                eth_log_delegation(2, delegator2, vec![v1], vec![s3]),
                eth_log_withdrawal(2, delegator2, 0), // amount does nothing
            ])
            .await;
        let mut db = DummyDb::filled();

        queue.commit_diffs(&mut db, 1).await;
        assert_eq!(db.data.lock().validators.get(&v1), Some(&(s1, None)),);
        assert_eq!(db.data.lock().validators.get(&v2), Some(&(s1, None)),);

        queue.commit_diffs(&mut db, 2).await;
        assert_eq!(db.data.lock().validators.get(&v1), Some(&(s1, None)),);
        assert_eq!(db.data.lock().validators.get(&v2), Some(&(0, None)),);
    }

    #[tokio::test]
    async fn test_reverting_pending_logs() {
        let mut rng = StdRng::seed_from_u64(3020);
        let v1: ValidatorId = rng.gen();
        let v2: ValidatorId = rng.gen();
        let c1: ConsensusId = rng.gen();
        let c2: ConsensusId = rng.gen();

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(10u64),
            BlockHeight::from(0u64),
        );

        let reg1 = eth_log_validator_registration(1, v1, c1);
        let mut reg1_revert = reg1.clone();
        reg1_revert.removed = Some(true);

        let reg2 = eth_log_validator_registration(2, v2, c2);
        let mut reg2_revert = reg2.clone();
        reg2_revert.removed = Some(true);

        let unreg1 = eth_log_validator_unregistration(0, v1);

        queue.append_eth_logs(vec![reg1, reg2]).await;

        assert_eq!(queue.pending.len(), 2);

        queue
            .append_eth_logs(vec![reg1_revert, reg2_revert, unreg1])
            .await;

        assert_eq!(queue.pending.len(), 1)
    }

    #[tokio::test]
    async fn test_reverting_pending_logs_on_new_block() {
        let mut rng = StdRng::seed_from_u64(3020);
        let v1: ValidatorId = rng.gen();
        let v2: ValidatorId = rng.gen();
        let c1: ConsensusId = rng.gen();
        let c2: ConsensusId = rng.gen();

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(10u64),
            BlockHeight::from(0u64),
        );

        let reg1 = eth_log_validator_registration(1, v1, c1);
        let mut reg1_revert = reg1.clone();
        reg1_revert.removed = Some(true);

        let reg2 = eth_log_validator_registration(2, v2, c2);
        let mut reg2_revert = reg2.clone();
        reg2_revert.removed = Some(true);

        queue.append_eth_logs(vec![reg1, reg2]).await;
        assert_eq!(queue.pending.len(), 2);

        queue.append_eth_logs(vec![reg1_revert, reg2_revert]).await;

        let mut db = DummyDb::filled();
        queue.commit_diffs(&mut db, 1).await;

        assert_eq!(queue.pending.len(), 0)
    }

    #[tokio::test]
    pub async fn simple_get_validator_set_down_drift() {
        let mut rng = StdRng::seed_from_u64(3020);
        let mut delegator1: Address = rng.gen();
        let mut delegator2: Address = rng.gen();
        let mut delegator3: Address = rng.gen();
        delegator1.iter_mut().take(12).for_each(|i| *i = 0);
        delegator2.iter_mut().take(12).for_each(|i| *i = 0);
        delegator3.iter_mut().take(12).for_each(|i| *i = 0);
        let mut v1: ValidatorId = rng.gen();
        let mut v2: ValidatorId = rng.gen();
        let cons1: ConsensusId = rng.gen();
        let cons2: ConsensusId = rng.gen();
        v1.iter_mut().take(12).for_each(|i| *i = 0);
        v2.iter_mut().take(12).for_each(|i| *i = 0);

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(0u64),
            BlockHeight::from(0u64),
        );

        let s1 = rng.gen::<u16>() as u64;
        let s2 = rng.gen::<u16>() as u64;
        let s3 = rng.gen::<u16>() as u64;

        queue
            .append_eth_logs(vec![
                eth_log_validator_registration(1, v1, cons1),
                eth_log_validator_registration(1, v2, cons2),
                eth_log_delegation(1, delegator1, vec![v1, v2], vec![s1, s2]),
                eth_log_delegation(2, delegator1, vec![v1], vec![s1]),
                eth_log_delegation(2, delegator2, vec![v2], vec![s1]),
                eth_log_delegation(3, delegator2, vec![v1], vec![s3]),
                eth_log_withdrawal(4, delegator2, 0),
            ])
            .await;
        let mut db = DummyDb::filled();

        // finalize all logs
        queue.commit_diffs(&mut db, 5).await;

        let set = queue.get_validators(6, &mut db).await;
        assert_eq!(set, None);

        let set = queue.get_validators(5, &mut db).await;
        assert!(set.is_some(), "Should be some for 5");
        let set = set.unwrap();
        assert_eq!(set.get(&v1), Some(&(s1, Some(cons1))));
        assert_eq!(set.get(&v2), None);

        let set = queue.get_validators(4, &mut db).await;
        assert!(set.is_some(), "Should be some for 4");
        let set = set.unwrap();
        assert_eq!(set.get(&v1), Some(&(s1, Some(cons1))));
        assert_eq!(set.get(&v2), None);

        let set = queue.get_validators(3, &mut db).await;
        assert!(set.is_some(), "Should be some for 3");
        let set = set.unwrap();
        assert_eq!(set.get(&v1), Some(&(s1 + s3, Some(cons1))));
        assert_eq!(set.get(&v2), None);

        let set = queue.get_validators(2, &mut db).await;
        assert!(set.is_some(), "Should be some for 2");
        let set = set.unwrap();
        assert_eq!(set.get(&v1), Some(&(s1, Some(cons1))));
        assert_eq!(set.get(&v2), Some(&(s1, Some(cons2))));

        let set = queue.get_validators(1, &mut db).await;
        assert!(set.is_some(), "Should be some for 1");
        let set = set.unwrap();
        assert_eq!(set.get(&v1), Some(&(s1, Some(cons1))));
        assert_eq!(set.get(&v2), Some(&(s2, Some(cons2))));
    }
}
