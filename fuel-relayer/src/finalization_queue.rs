use crate::{
    log::{AssetDepositLog, EthEventLog},
    pending_blocks::PendingBlocks,
    validators::Validators,
};
use ethers_core::types::{Log, H160};
use ethers_providers::Middleware;
use fuel_core_interfaces::{
    model::{BlockHeight, DaBlockHeight, SealedFuelBlock},
    relayer::{RelayerDb, StakingDiff},
};
use fuel_tx::{Address, Bytes32};
use std::{
    cmp::min,
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tracing::{debug, error, info, warn};

pub struct FinalizationQueue {
    /// Pending stakes/assets/withdrawals. Before they are finalized
    pending: VecDeque<DaBlockDiff>,
    /// most recently finalized da height
    finalized_da_height: DaBlockHeight,
    /// Pending block handling
    blocks: PendingBlocks,
    /// Current validator set
    validators: Validators,
    /// Track how far back an ongoing reorg is occurring.
    lowest_da_reorg_height: DaBlockHeight,
}

/// Pending diff between FuelBlocks
#[derive(Clone, Debug, Default)]
pub struct DaBlockDiff {
    /// da block height
    pub da_height: DaBlockHeight,
    /// Validator stake deposit and withdrawel.
    pub validators: HashMap<Address, Option<Address>>,
    // Delegation diff contains new delegation list, if we did just withdrawal option will be None.
    pub delegations: HashMap<Address, Option<HashMap<Address, u64>>>,
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
        last_commited_finalized_fuel_height: BlockHeight,
    ) -> Self {
        let blocks = PendingBlocks::new(
            chain_id,
            contract_address,
            private_key,
            last_commited_finalized_fuel_height,
        );
        Self {
            blocks,
            pending: VecDeque::new(),
            validators: Validators::default(),
            finalized_da_height: 0,
            lowest_da_reorg_height: u32::MAX,
        }
    }

    pub async fn load_validators(&mut self, db: &dyn RelayerDb) {
        self.validators.load(db).await
    }

    pub async fn get_validators(
        &mut self,
        da_height: DaBlockHeight,
    ) -> Option<HashMap<Address, (u64, Option<Address>)>> {
        self.validators.get(da_height).await
    }

    pub fn clear(&mut self) {
        self.pending.clear()
    }

    /// propagate new fuel block to pending_blocks
    pub fn handle_fuel_block(&mut self, block: &Arc<SealedFuelBlock>) {
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
        // accumulate how far back a reorg goes.
        if removed {
            self.lowest_da_reorg_height = min(self.lowest_da_reorg_height, da_height);
            return;
        }
        // apply all reverted event
        if self.lowest_da_reorg_height != DaBlockHeight::MAX {
            info!(
                "Reorg happened on ethereum. Reverting to block {}",
                self.lowest_da_reorg_height
            );
            // remove all blocks that were reverted. In best case those blocks heights and events are going
            // to be reinserted in append eth events.
            self.blocks
                .revert_blocks_after_height(self.lowest_da_reorg_height);
            self.pending
                .retain(|diff| diff.da_height < self.lowest_da_reorg_height);
            // reset reorg height after reverting blocks
            self.lowest_da_reorg_height = DaBlockHeight::MAX;
        }
        // apply new event to pending queue
        self.append_da_events(event, da_height).await;
    }

    /// At beginning we will ignore all event until event for new fuel block commit comes
    /// after that synchronization can start.
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
                self.blocks
                    .handle_block_commit(block_root, (height).into(), da_height);
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
    use fuel_core_interfaces::db::helpers::DummyDb;
    use fuel_types::{Address, AssetId};
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
        let v1: Address = rng.gen();
        let v2: Address = rng.gen();
        let c1: Address = rng.gen();
        let c2: Address = rng.gen();

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(0u64),
        );

        let v1_register = eth_log_validator_registration(0, v1, c1);
        let v2_register = eth_log_validator_registration(0, v2, c2);
        let v1_unregister = eth_log_validator_unregistration(1, v1);

        queue
            .append_eth_logs(vec![v1_register, v2_register, v1_unregister])
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
        let v1: Address = rng.gen();
        let c1: Address = rng.gen();
        let v2: Address = rng.gen();
        let c2: Address = rng.gen();

        let acc1: Address = rng.gen();
        let token1 = AssetId::zeroed();
        let nonce1: Bytes32 = rng.gen();

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(0u64),
        );

        let v1_register = eth_log_validator_registration(1, v1, c1);
        let v2_register = eth_log_validator_registration(2, v2, c2);
        let deposit1 = eth_log_asset_deposit(2, acc1, token1, 1, 40, nonce1, 0);
        let v1_unregister = eth_log_validator_unregistration(3, v1);

        queue
            .append_eth_logs(vec![v1_register, v2_register, deposit1, v1_unregister])
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
        let mut v1: Address = rng.gen();
        let c1: Address = rng.gen();
        let mut v2: Address = rng.gen();
        v1.iter_mut().take(12).for_each(|i| *i = 0);
        v2.iter_mut().take(12).for_each(|i| *i = 0);

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(0u64),
        );

        let s1 = rng.gen::<u16>() as u64;
        let s2 = rng.gen::<u16>() as u64;
        let s3 = rng.gen::<u16>() as u64;

        let del1 = eth_log_delegation(1, delegator1, vec![v1, v2], vec![s1, s2]);
        let v1_register = eth_log_validator_registration(2, v1, c1);
        let del2 = eth_log_delegation(2, delegator2, vec![v1], vec![s3]);
        let del_with = eth_log_withdrawal(3, delegator1, 7);

        queue
            .append_eth_logs(vec![del1, del2, v1_register, del_with])
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
        let mut v1: Address = rng.gen();
        let mut v2: Address = rng.gen();
        v1.iter_mut().take(12).for_each(|i| *i = 0);
        v2.iter_mut().take(12).for_each(|i| *i = 0);

        let mut queue = FinalizationQueue::new(
            0,
            H160::zero(),
            &(hex::decode("79afbf7147841fca72b45a1978dd7669470ba67abbe5c220062924380c9c364b")
                .unwrap()),
            BlockHeight::from(0u64),
        );

        let s1 = rng.gen::<u16>() as u64;
        let s2 = rng.gen::<u16>() as u64;
        let s3 = rng.gen::<u16>() as u64;

        let del1 = eth_log_delegation(1, delegator1, vec![v1, v2], vec![s1, s2]);
        let del1_ret = eth_log_delegation(1, delegator1, vec![v1], vec![s1]);
        let del2 = eth_log_delegation(1, delegator2, vec![v2], vec![s1]);

        let del2_first = eth_log_delegation(2, delegator2, vec![v1], vec![s3]);
        let del2_ret = eth_log_withdrawal(2, delegator2, 0); // amount does nothing

        queue
            .append_eth_logs(vec![del1, del1_ret, del2, del2_first, del2_ret])
            .await;
        let mut db = DummyDb::filled();

        queue.commit_diffs(&mut db, 1).await;
        assert_eq!(db.data.lock().validators.get(&v1), Some(&(s1, None)),);
        assert_eq!(db.data.lock().validators.get(&v2), Some(&(s1, None)),);

        queue.commit_diffs(&mut db, 2).await;
        assert_eq!(db.data.lock().validators.get(&v1), Some(&(s1, None)),);
        assert_eq!(db.data.lock().validators.get(&v2), Some(&(0, None)),);
    }
}
