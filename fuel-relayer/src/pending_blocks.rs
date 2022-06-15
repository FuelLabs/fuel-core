use anyhow::Error;
use ethers_core::{
    k256::ecdsa::SigningKey,
    types::{TransactionRequest, H160, U256},
};
use ethers_middleware::{
    gas_escalator::{Frequency, GasEscalatorMiddleware, GeometricGasPrice},
    NonceManagerMiddleware, SignerMiddleware,
};
use ethers_providers::Middleware;
use fuel_core_interfaces::{
    common::fuel_tx::Bytes32,
    model::{BlockHeight, DaBlockHeight, SealedFuelBlock},
    relayer::RelayerDb,
};
use std::{cmp::max, collections::VecDeque, sync::Arc};

// use the ethers_signers crate to manage LocalWallet and Signer
use crate::abi;
use ethers_signers::{LocalWallet, Signer};
use tracing::{debug, error, info, warn};

/// Pending Fuel Blocks waiting to be finalized inside client. Until then
/// there is possibility that they are going to be reverted
pub struct PendingBlocks {
    signer: LocalWallet,
    contract_address: H160,
    /// Pending block commits seen on DA layer and waiting to be finalized
    pending_block_commits: VecDeque<PendingBlock>,
    /// Highest known chain height, used to check if we are seeing lag between block commits and our fule chain
    chain_height: BlockHeight,
    /// Last known commited and finalized fuel height that is known by client.
    last_commited_finalized_fuel_height: BlockHeight,
}

struct PendingBlock {
    pub reverted: bool,
    pub da_height: DaBlockHeight,
    pub block_height: BlockHeight,
    pub block_root: Bytes32, //is this block hash?
}

impl PendingBlock {
    pub fn new_commited_block(
        da_height: DaBlockHeight,
        block_height: BlockHeight,
        block_root: Bytes32,
    ) -> Self {
        Self {
            reverted: false,
            da_height,
            block_height,
            block_root,
        }
    }
}

pub fn from_fuel_to_block_header(fuel_block: &SealedFuelBlock) -> abi::fuel::BlockHeader {
    let block = abi::fuel::BlockHeader {
        producer: H160::from_slice(&fuel_block.header.producer.as_ref()[12..]),
        previous_block_root: <[u8; 32]>::try_from(fuel_block.id()).unwrap(),
        height: fuel_block.header.height.into(),
        block_number: fuel_block.header.number.into(), // TODO
        digest_root: [0; 32],
        digest_hash: [0; 32],
        digest_length: 0,
        transaction_root: <[u8; 32]>::try_from(fuel_block.header.transactions_root.as_ref())
            .unwrap(),
        transaction_sum: fuel_block.transaction_sum().into(),
        num_transactions: fuel_block.transactions.len() as u32,
        validator_set_hash: <[u8; 32]>::try_from(fuel_block.validator_set_hash().as_ref()).unwrap(),
        required_stake: fuel_block.consensus.required_stake.into(),
        withdrawals_root: <[u8; 32]>::try_from(fuel_block.withdrawals_root().as_ref()).unwrap(),
        transactions_data_length: 0,
        transaction_hash: <[u8; 32]>::try_from(fuel_block.transaction_data_hash().as_ref())
            .unwrap(),
    };
    block
}

pub enum IsReverted {
    True,
    False,
}

impl PendingBlocks {
    /// Pending blocks at least finalization number of blocks.
    pub fn new(
        chain_id: u64,
        contract_address: H160,
        private_key: &[u8],
        chain_height: BlockHeight,
        last_commited_finalized_fuel_height: BlockHeight,
    ) -> Self {
        // it is some random key for now
        let sk = SigningKey::from_bytes(private_key).unwrap();
        let signer: LocalWallet = sk.into();
        let signer = signer.with_chain_id(chain_id);

        Self {
            signer,
            contract_address,
            chain_height,
            pending_block_commits: VecDeque::new(),
            last_commited_finalized_fuel_height,
        }
    }

    /// new sealed fuel block received update chain_height
    pub fn set_chain_height(&mut self, height: BlockHeight) {
        self.chain_height = height;
    }

    /// Discard block from pending queue that got finalized.
    /// return last finalized block commit hash and height if there is one.
    pub fn handle_da_finalization(&mut self, finalized_da_height: DaBlockHeight) -> BlockHeight {
        // iterate over all pending blocks and finalize some\
        self.pending_block_commits.retain(|block| {
            if block.da_height <= finalized_da_height {
                self.last_commited_finalized_fuel_height = BlockHeight::from(max(
                    u64::from(block.block_height),
                    u64::from(self.last_commited_finalized_fuel_height),
                ));

                false
            } else {
                true
            }
        });
        self.last_commited_finalized_fuel_height
    }

    pub fn handle_block_commit(
        &mut self,
        block_root: Bytes32,
        height: BlockHeight,
        da_height: DaBlockHeight,
        is_reverted: IsReverted,
    ) {
        match is_reverted {
            IsReverted::True => self.handle_block_commit_revert(block_root, height, da_height),
            IsReverted::False => self.handle_block_commit_append(block_root, height, da_height),
        }
    }

    async fn bundle(
        &mut self,
        to_height: BlockHeight,
        db: &mut dyn RelayerDb,
    ) -> Vec<Arc<SealedFuelBlock>> {
        // if queue is empty check last_finalized_commited fuel block and send all newest ones that we know about.
        let mut from_height = self.last_commited_finalized_fuel_height;
        //get blocks range that start from last pending queue item that is not reverted and goes to current block.
        for pending in self.pending_block_commits.iter() {
            if !pending.reverted {
                from_height = pending.block_height;
                break;
            }
        }

        debug!("Bundle from:{from_height}, to:{to_height}");
        let mut bundle = Vec::new();
        for height in from_height.as_usize()..=to_height.as_usize() {
            if let Some(sealed_block) = db.get_sealed_block(BlockHeight::from(height)).await {
                bundle.push(sealed_block.clone());
            } else {
                panic!("All not commited blocks should have its seal and blocks inside db");
            }
        }
        bundle
    }

    /// When new block is created by this client, bundle all not commited blocks and send it to contract.
    pub async fn commit<P>(
        &mut self,
        height: BlockHeight,
        db: &mut dyn RelayerDb,
        provider: &Arc<P>,
    ) where
        P: Middleware + 'static,
    {
        self.set_chain_height(height);
        debug!("Handle new created_block {}", height);

        let mut bundle = self.bundle(height, db).await.into_iter();

        let mut parent = if let Some(first_parent) = bundle.next() {
            first_parent
        } else {
            panic!("First Parent should be present:{}", height);
        };

        for block in bundle {
            info!(
                "Bundle send pair {}:{} of blocks:",
                parent.header.height, block.header.height
            );
            if let Err(error) = self.call_contract(&parent, &block, provider).await {
                warn!("Commit fuel block failed: {}", error);
                break;
            }
            parent = block;
        }
    }

    /// Handle commited block from contract.
    fn handle_block_commit_append(
        &mut self,
        block_root: Bytes32,
        height: BlockHeight,
        da_height: DaBlockHeight,
    ) {
        if self.pending_block_commits.is_empty() {
            let lcffh = self.last_commited_finalized_fuel_height;
            if lcffh + 1u64.into() != height {
                error!("Missing block commitments between last finalized commitment {lcffh} to new height {height}")
            }
            // no pending commits, add one
            self.pending_block_commits
                .push_front(PendingBlock::new_commited_block(
                    da_height, height, block_root,
                ));
            return;
        }

        let front_height = self.pending_block_commits.front().unwrap().block_height;
        let back_height = self.pending_block_commits.back().unwrap().block_height;

        if height < back_height {
            // This case means that we somehow skipped block commit and didnt receive it in expected order.
            error!(
                "Commited block {height} is lower then current lowest pending block {back_height}."
            );
        } else if height > front_height {
            // check if we are lagging against da layer
            if self.chain_height < height {
                error!(
                    "Our chain height: {} is lower then da layer height {height}",
                    self.chain_height
                );
            }
            // new block received. Happy path
            if height == front_height + BlockHeight::from(1u64) {
                self.pending_block_commits
                    .push_front(PendingBlock::new_commited_block(
                        da_height, height, block_root,
                    ));
            } else {
                error!("Commited block height {height} should be only increased by one from current height {front_height}.");
            }
        } else {
            // happens if reverted commit is again visible
            // iterate over pending blocks and set reverted commit as commited.
            for pending in self.pending_block_commits.iter_mut() {
                if pending.block_height == height {
                    if !pending.reverted {
                        error!("We received block {height} commit that was not reverted")
                    }
                    pending.da_height = da_height;
                    pending.reverted = false;
                    pending.block_root = block_root;
                    break;
                }
            }
        }
    }

    /// Handle revert of block commit from contract.
    fn handle_block_commit_revert(
        &mut self,
        block_root: Bytes32,
        height: BlockHeight,
        da_height: DaBlockHeight,
    ) {
        if self.pending_block_commits.is_empty() {
            // nothing to revert
            error!("Revert for height {height} received while pending block queue is empty and LFCFB is {}",
                self.last_commited_finalized_fuel_height);
            return;
        }

        let front_height = self.pending_block_commits.front().unwrap().block_height;
        let back_height = self.pending_block_commits.back().unwrap().block_height;

        if height < back_height {
            // ignore block that are not inside pending queue.
            error!("All pending block commits should be present in block queue. height:{height} last_known:{back_height}");
        } else if height > front_height {
            error!("Something unexpected happened.Reverted block commits are not something found in the future.");
        } else {
            // happy path. iterate over pending blocks and set it as commited.
            for pending in self.pending_block_commits.iter_mut() {
                if pending.block_height == height {
                    if pending.reverted {
                        error!("We received block {height} commit that was already reverted");
                    }
                    pending.da_height = da_height;
                    pending.reverted = true;
                    pending.block_root = block_root;
                }
            }
        }
    }

    async fn call_contract<P>(
        &self,
        parent: &SealedFuelBlock,
        block: &SealedFuelBlock,
        provider: &Arc<P>,
    ) -> Result<(), Error>
    where
        P: Middleware + 'static,
    {
        let wrapped_block = from_fuel_to_block_header(block);
        let wrapped_parent = from_fuel_to_block_header(parent);

        let validators = block
            .consensus
            .validators
            .iter()
            .map(|(val, _)| H160::from_slice(&val.as_ref()[12..])) // TODO check if this needs to do keccak then 12..
            .collect();
        let stakes = block
            .consensus
            .validators
            .iter()
            .map(|(_, (stake, _))| (*stake).into())
            .collect(); // U256
        let signatures = block
            .consensus
            .validators
            .iter()
            .map(|(_, (_, sig))| sig.to_vec().into())
            .collect(); //bytes
        let withdrawals = block
            .withdrawals()
            .iter()
            .map(|wd| abi::fuel::Withdrawal {
                owner: H160::from_slice(&wd.0.as_ref()[12..]),
                token: H160::from_slice(&wd.2.as_ref()[12..]),
                amount: wd.1.into(),
                precision: 0,
                nonce: U256::zero(),
            })
            .collect();

        let calldata = {
            let contract = abi::Fuel::new(self.contract_address, provider.clone());
            let event = contract.commit_block(
                block.header.height.into(),
                <[u8; 32]>::try_from(block.id()).unwrap(),
                wrapped_block,
                wrapped_parent,
                validators,
                stakes,
                signatures,
                withdrawals,
            );
            //
            event.calldata().expect("To have caldata")
        };

        // Escalate gas prices
        let escalator = GeometricGasPrice::new(1.125, 60u64, None::<u64>);
        let provider =
            GasEscalatorMiddleware::new(provider.clone(), escalator, Frequency::PerBlock);

        // Sign transactions with a private key
        let address = self.signer.address();
        let provider = SignerMiddleware::new(provider, self.signer.clone());

        // Use EthGasStation as the gas oracle
        // https://github.com/FuelLabs/fuel-core/issues/363
        // TODO check how this is going to be done in testnet.
        //let gas_oracle = EthGasStation::new(None);
        //let provider = GasOracleMiddleware::new(provider, gas_oracle);

        // Manage nonces locally
        let provider = NonceManagerMiddleware::new(provider, address);

        // craft the tx
        let tx = TransactionRequest::new()
            .from(address)
            .to(self.contract_address)
            .gas_price(20000000001u64)
            .data(calldata);
        let _ = provider.send_transaction(tx, None).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use fuel_core_interfaces::db::helpers::DummyDb;
    use rand::{prelude::StdRng, Rng, SeedableRng};

    use super::*;
    use tracing_test::traced_test;

    pub fn block_commit(last_commited_fuel_block: BlockHeight) -> PendingBlocks {
        let private_key =
            hex::decode("c6bd905dcac2a0b1c43f574ab6933df14d7ceee0194902bce523ed054e8e798b")
                .unwrap();
        PendingBlocks::new(
            0,
            H160::zero(),
            &private_key,
            10u64.into(),
            last_commited_fuel_block,
        )
    }

    #[test]
    pub fn test_simple_log_append_with_finalizations() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();
        let b2 = rng.gen();
        let b3 = rng.gen();
        let b4 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::False);
        blocks.handle_block_commit(b2, 3u64.into(), 10, IsReverted::False);
        blocks.handle_block_commit(b3, 4u64.into(), 11, IsReverted::False);
        blocks.handle_block_commit(b4, 5u64.into(), 13, IsReverted::False);
        blocks.handle_block_commit(b4, 5u64.into(), 13, IsReverted::True);

        let q = &blocks.pending_block_commits;
        assert_eq!(q.len(), 4, "Should contain for pending blocks");
        blocks.handle_da_finalization(10);

        let q = &blocks.pending_block_commits;
        assert_eq!(q.len(), 2, "Should contains only two pending blocks");

        let back = q.back().unwrap();
        let front = q.front().unwrap();
        assert_eq!(back.block_root, b3, "First back should be b3");
        assert_eq!(front.block_root, b4, "First front should be b4");
    }

    #[test]
    #[traced_test]
    pub fn error_log_on_lower_block_commit() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();
        let b2 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::False);
        blocks.handle_block_commit(b2, 0u64.into(), 9, IsReverted::False);
        assert!(logs_contain(
            "Commited block 0 is lower then current lowest pending block 2."
        ))
    }

    #[test]
    #[traced_test]
    pub fn error_log_on_higher_block_commit() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();
        let b2 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::False);
        blocks.handle_block_commit(b2, 4u64.into(), 10, IsReverted::False);
        assert!(logs_contain(
            "Commited block height 4 should be only increased by one from current height 2"
        ))
    }

    #[test]
    #[traced_test]
    pub fn duplicated_log_received_for_block_commit() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();
        let b2 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::False);
        blocks.handle_block_commit(b2, 2u64.into(), 10, IsReverted::False);
        assert!(logs_contain(
            "We received block 2 commit that was not reverted"
        ))
    }

    #[test]
    #[traced_test]
    pub fn skipped_logs_for_new_block_commit_on_empty_queue() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 3u64.into(), 9, IsReverted::False);
        assert!(logs_contain(
            "Missing block commitments between last finalized commitment 1 to new height 3"
        ))
    }

    #[test]
    #[traced_test]
    pub fn error_log_on_lower_block_commit_revert() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();
        let b2 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::False);
        blocks.handle_block_commit(b2, 3u64.into(), 9, IsReverted::False);
        blocks.handle_block_commit(b2, 1u64.into(), 9, IsReverted::True);
        assert!(logs_contain(
            "All pending block commits should be present in block queue. height:1 last_known:2"
        ))
    }

    #[test]
    #[traced_test]
    pub fn error_log_on_higher_block_commit_revert() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();
        let b2 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::False);
        blocks.handle_block_commit(b2, 3u64.into(), 10, IsReverted::True);
        assert!(logs_contain(
            "Something unexpected happened.Reverted block commits are not something found in the future"
        ))
    }

    #[test]
    #[traced_test]
    pub fn duplicated_log_received_for_block_commit_revert() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::False);
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::True);
        blocks.handle_block_commit(b1, 2u64.into(), 9, IsReverted::True);
        assert!(logs_contain(
            "We received block 2 commit that was already reverted"
        ))
    }

    #[test]
    #[traced_test]
    pub fn skipped_logs_for_block_commit_revert_on_empty_queue() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        blocks.handle_block_commit(b1, 10u64.into(), 9, IsReverted::True);
        assert!(logs_contain(
            "Revert for height 10 received while pending block queue is empty and LFCFB is 1"
        ))
    }

    #[tokio::test]
    async fn bundle_on_empty_pending_queue() {
        let mut blocks = block_commit(1u64.into());
        let mut db = Box::new(DummyDb::filled());

        let out = blocks.bundle(3u64.into(), db.as_mut()).await;
        assert_eq!(out.len(), 3, "We should have bundled 3 blocks");
        assert_eq!(out[0].header.height, 1u64.into(), "First should be 1");
        assert_eq!(out[1].header.height, 2u64.into(), "Seocnd should be 2");
        assert_eq!(out[2].header.height, 3u64.into(), "Third should be 3");
    }

    #[tokio::test]
    async fn bundle_on_one_block_in_queue() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        let mut db = Box::new(DummyDb::filled());
        blocks.handle_block_commit(b1, 2u64.into(), 2, IsReverted::False);

        let out = blocks.bundle(3u64.into(), db.as_mut()).await;
        assert_eq!(out.len(), 2, "We should have bundled 2 blocks");
        assert_eq!(out[0].header.height, 2u64.into(), "First should be 2");
        assert_eq!(out[1].header.height, 3u64.into(), "Second should be 3");
    }

    #[tokio::test]
    #[should_panic(expected = "All not commited blocks should have its seal and blocks inside db")]
    async fn bundle_should_panic_if_sealed_block_is_missing() {
        let mut blocks = block_commit(1u64.into());
        let mut db = Box::new(DummyDb::filled());

        blocks.bundle(10u64.into(), db.as_mut()).await;
    }

    #[tokio::test]
    async fn bundle_on_one_block_and_one_revert() {
        let mut rng = StdRng::seed_from_u64(59);

        let b1 = rng.gen();
        let b2 = rng.gen();

        let mut blocks = block_commit(1u64.into());
        let mut db = Box::new(DummyDb::filled());
        blocks.handle_block_commit(b1, 2u64.into(), 2, IsReverted::False);
        blocks.handle_block_commit(b2, 3u64.into(), 3, IsReverted::False);
        blocks.handle_block_commit(b2, 3u64.into(), 3, IsReverted::True);

        let out = blocks.bundle(4u64.into(), db.as_mut()).await;
        assert_eq!(out.len(), 3, "We should have bundled 3 blocks");
        assert_eq!(out[0].header.height, 2u64.into(), "First should be 2");
        assert_eq!(out[1].header.height, 3u64.into(), "First should be 3");
        assert_eq!(out[2].header.height, 4u64.into(), "Second should be 4");
    }
}
