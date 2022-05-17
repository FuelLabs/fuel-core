use std::{cmp::max, collections::VecDeque, sync::Arc};

use anyhow::Error;
use ethers_contract::*;
use ethers_core::{
    k256::ecdsa::SigningKey,
    types::{TransactionRequest, H160, U256},
};
use ethers_middleware::{
    gas_escalator::{Frequency, GasEscalatorMiddleware, GeometricGasPrice},
    NonceManagerMiddleware, SignerMiddleware,
};
use ethers_providers::Middleware;
use fuel_tx::Bytes32;

use fuel_core_interfaces::{
    model::{BlockHeight, SealedFuelBlock},
    relayer::RelayerDb,
};

// use the ethers_signers crate to manage LocalWallet and Signer
use ethers_signers::{LocalWallet, Signer};
use tracing::{debug, error, info, warn};

abigen!(ContractAbi, "abi/fuel.json");

pub struct BlockCommit {
    signer: LocalWallet,
    contract_address: H160,
    // Pending block commits seen on DA layer and waiting to be finalized
    pending_block_commits: VecDeque<PendingBlock>,
    // Highest known chain height, used to check if we are seeing lag between block commits and our fule chain
    chain_height: BlockHeight,
    // Last known commited and finalized fuel height that is known by client.
    last_commited_finalized_fuel_height: BlockHeight,
}

pub struct PendingBlock {
    pub reverted: bool,
    pub da_height: BlockHeight,
    pub block_height: BlockHeight,
    pub block_root: Bytes32, //is this block hash?
}

impl PendingBlock {
    pub fn new_commited_block(
        da_height: BlockHeight,
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

pub fn from_fuel_to_block_header(fuel_block: &SealedFuelBlock) -> BlockHeader {
    let block = BlockHeader {
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

impl BlockCommit {
    /// Pending blocks at least finalization number of blocks.
    pub fn new(
        chain_id: u64,
        contract_address: H160,
        private_key: Vec<u8>,
        last_commited_finalized_fuel_height: BlockHeight,
    ) -> Self {
        // it is some random key for now
        let sk = SigningKey::from_bytes(&private_key).unwrap();
        let signer: LocalWallet = sk.into();
        let signer = signer.with_chain_id(chain_id);

        Self {
            signer,
            contract_address,
            chain_height: BlockHeight::from(10u64), // TODO
            pending_block_commits: VecDeque::new(),
            last_commited_finalized_fuel_height,
        }
    }

    pub fn last_commited_finalized_fuel_height(&self) -> BlockHeight {
        self.last_commited_finalized_fuel_height
    }

    /// Discard block from pending queue that got finalized.
    /// return last finalized block commit hash and height if there is one.
    pub fn new_da_block(&mut self, finalized_da_height: BlockHeight) {
        // iterate over all pending blocks and finalize some
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
    }

    /// new sealed fuel block received update chain_height
    pub fn set_chain_height(&mut self, height: BlockHeight) {
        self.chain_height = height;
    }

    /// Handle commited block from contract.
    pub fn block_commited(
        &mut self,
        block_root: Bytes32,
        height: BlockHeight,
        da_height: BlockHeight,
    ) {
        if self.pending_block_commits.is_empty() {
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
            panic!(
                "Commited block {} is lower then current lowest pending block {}.",
                height, back_height
            );
        } else if height > front_height {
            // check if we are lagging agains da layer
            if self.chain_height < height {
                error!(
                    "Our chain height: {} is lower then da layer height {}",
                    self.chain_height, height
                );
            }
            // new block received. Happy path
            if height == front_height + BlockHeight::from(1u64) {
                self.pending_block_commits
                    .push_front(PendingBlock::new_commited_block(
                        da_height, height, block_root,
                    ));
            } else {
                panic!("Something unexpected happened. New Fuel commits are always increased by one, it cants jump numbers ")
            }
        } else {
            // happens if reverted commit is again visible
            // iterate over pending blocks and set reverted commit as commited.
            for pending in self.pending_block_commits.iter_mut() {
                if pending.block_height == height {
                    pending.da_height = da_height;
                    pending.reverted = false;
                    pending.block_height = height;
                    pending.block_root = block_root;
                    break;
                }
            }
        }
    }

    /// Handle revert of block commit from contract.
    pub fn block_commit_reverted(
        &mut self,
        block_root: Bytes32,
        height: BlockHeight,
        da_height: BlockHeight,
    ) {
        // re-add block from bundle to be send to contract
        // handle commited block from contract, remove it from bundle
        if self.pending_block_commits.is_empty() {
            // nothing to revert
            return;
        }

        let front_height = self.pending_block_commits.front().unwrap().block_height;
        let back_height = self.pending_block_commits.back().unwrap().block_height;

        if height < back_height {
            // ignore block that are not inside pending queue.
            error!("All pending block commits should be present in block queue. height:{} last_known:{}"
            ,height,back_height);
        } else if height > front_height {
            error!("Something unexpected happened.Reverted block commits are not something found in the future.");
        } else {
            // happy path. iterate over pending blocks and set it as commited.
            for pending in self.pending_block_commits.iter_mut() {
                if pending.block_height > height {
                    pending.da_height = da_height;
                    pending.reverted = true;
                    pending.block_height = height;
                    pending.block_root = block_root;
                }
            }
        }
    }

    /// When new block is created by this client, bundle all not commited blocks and send it to contract.
    pub async fn created_block<P>(
        &mut self,
        block: Arc<SealedFuelBlock>,
        db: &mut dyn RelayerDb,
        provider: &Arc<P>,
    ) where
        P: Middleware + 'static,
    {
        debug!("Handle new created_block {}", block.header.height);
        self.set_chain_height(block.header.height);

        // if queue is empty check last_finalized_commited fuel block and send all newest ones that we know about.
        let mut from_height = self.last_commited_finalized_fuel_height;
        //get blocks range that start from last pending queue item that is not reverted and goes to current block.
        for pending in self.pending_block_commits.iter() {
            if !pending.reverted {
                from_height = pending.block_height;
                break;
            }
        }

        debug!("Bundle from:{}, to:{}", from_height, block.header.height);
        let mut parent = if let Some(parent) = db.get_sealed_block(from_height).await {
            parent
        } else {
            panic!("Parent should be present:{}", from_height);
        };
        //.expect("This block should be present as we couldn't create new block");
        from_height = from_height + BlockHeight::from(1u64);
        let mut bundle = Vec::new();
        for height in from_height.as_usize()..=block.header.height.as_usize() {
            if let Some(sealed_block) = db.get_sealed_block(BlockHeight::from(height)).await {
                bundle.push(sealed_block.clone());
            } else {
                panic!("All not commited blocks should have its seal and blocks inside db");
            }
        }

        for block in bundle.into_iter() {
            info!(
                "Bundle send pair {}:{} of blocks:",
                parent.header.height, block.header.height
            );
            if let Err(error) = self.commit_fuel_block(&parent, &block, provider).await {
                warn!("Commit fuel block failed: {}", error);
                break;
            }
            parent = block;
        }
    }

    pub async fn commit_fuel_block<P>(
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
            .map(|val| H160::from_slice(&val.as_ref()[12..])) // TODO check if this needs to do keccak then 12..
            .collect();
        let stakes = block
            .consensus
            .stakes
            .iter()
            .map(|stake| (*stake).into())
            .collect(); // U256
        let signatures = block
            .consensus
            .signatures
            .iter()
            .map(|sig| sig.to_vec().into())
            .collect(); //bytes
        let withdrawals = block
            .withdrawals()
            .iter()
            .map(|wd| Withdrawal {
                owner: H160::from_slice(&wd.0.as_ref()[12..]),
                token: H160::from_slice(&wd.2.as_ref()[12..]),
                amount: wd.1.into(),
                precision: 0,
                nonce: U256::zero(),
            })
            .collect();

        let calldata = {
            let contract = ContractAbi::new(self.contract_address, provider.clone());
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

    #[test]
    pub fn testing() {}
}
