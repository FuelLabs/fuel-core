use std::{collections::VecDeque, sync::Arc};

use anyhow::Error;
use ethers_contract::*;
use ethers_core::{
    k256::ecdsa::SigningKey,
    types::{TransactionRequest, H160, U256},
};
use ethers_middleware::{
    gas_escalator::{Frequency, GasEscalatorMiddleware, GeometricGasPrice},
    gas_oracle::{EthGasStation, GasOracleMiddleware},
    NonceManagerMiddleware, SignerMiddleware,
};
use ethers_providers::Middleware;
use fuel_tx::Bytes32;

use fuel_core_interfaces::model::{BlockHeight, SealedFuelBlock};

// use the ethers_signers crate to manage LocalWallet and Signer
use ethers_signers::{LocalWallet, Signer};
use tracing::error;

abigen!(ContractAbi, "abi/fuel.json");

pub struct BlockCommit {
    signer: LocalWallet,
    contract_address: H160,
    pending_blocks: VecDeque<PendingBlock>,
    last_finalized_block_commit: Option<Arc<SealedFuelBlock>>,
}

pub struct PendingBlock {
    pub fuel_block: Option<Arc<SealedFuelBlock>>,
    pub reverted: bool,
    pub da_height: Option<BlockHeight>,
    pub block_height: BlockHeight,
    pub block_root: Bytes32, //is this block hash?
}

impl PendingBlock {
    pub fn new_fuel_block(fuel_block: Arc<SealedFuelBlock>) -> Self {
        let block_height = fuel_block.header.height;
        let block_root = fuel_block.header.id();
        Self {
            fuel_block: Some(fuel_block.clone()),
            reverted: false,
            da_height: None,
            block_height,
            block_root,
        }
    }

    pub fn new_commited_block(
        da_height: BlockHeight,
        block_height: BlockHeight,
        block_root: Bytes32,
    ) -> Self {
        Self {
            fuel_block: None,
            reverted: false,
            da_height: Some(da_height),
            block_height,
            block_root,
        }
    }
}

pub fn from_fuel_to_block_header(fuel_block: &SealedFuelBlock) -> BlockHeader {
    let block = BlockHeader {
        producer: H160::from_slice(fuel_block.header.producer.as_ref()),
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
        transactions_data_length: fuel_block.transaction_data_lenght() as u32,
        transaction_hash: <[u8; 32]>::try_from(fuel_block.transaction_data_hash().as_ref())
            .unwrap(),
    };
    block
}

impl BlockCommit {
    /// Pending blocks at least finalization number of blocks.
    pub fn new(chain_id: u64, contract_address: H160, private_key: Vec<u8>) -> Self {
        // it is some random key for now
        let sk = SigningKey::from_bytes(&private_key).unwrap();
        let signer: LocalWallet = sk.into();
        let signer = signer.with_chain_id(chain_id);

        Self {
            signer,
            contract_address,
            pending_blocks: VecDeque::new(),
            last_finalized_block_commit: None,
        }
    }

    /// Discard block from pending queue that got finalized.
    /// return last finalized block comit hash if there is one.
    pub fn new_da_block(
        &mut self,
        finalized_da_height: BlockHeight,
    ) -> Option<(BlockHeight, Bytes32)> {
        let mut last_commited_block = None;
        // iterate over all pending blocks and finalize some
        self.pending_blocks.retain(|block| {
            if let Some(da_height) = block.da_height {
                if da_height <= finalized_da_height {
                    last_commited_block = if let Some((height, _)) = last_commited_block {
                        // if there is better finalized block, use it.
                        if block.block_height > height {
                            Some((block.block_height, block.block_root))
                        } else {
                            last_commited_block
                        }
                    } else {
                        Some((block.block_height, block.block_root))
                    };

                    // remove it from pending
                    return false;
                }
            }
            return true;
        });
        return last_commited_block;
    }

    /// Append new fuel block into pending queue.
    pub fn new_fuel_block(&mut self, block: Arc<SealedFuelBlock>) {
        if self.pending_blocks.is_empty() {
            // pending block queue is empty, add new block at front
            self.pending_blocks
                .push_front(PendingBlock::new_fuel_block(block));
            return;
        }
        let front_height = u64::from(self.pending_blocks.front().unwrap().block_height);
        let back_height = u64::from(self.pending_blocks.back().unwrap().block_height);
        let block_height = u64::from(block.header.height);
        if block_height < back_height {
            // do nothing this block height was already commited and finalized on contract side.
        } else if block_height > front_height {
            // expected thing to happen
            if block_height == front_height + 1 {
                // happy path
                self.pending_blocks
                    .push_front(PendingBlock::new_fuel_block(block));
            } else {
                panic!("Something unexpected happened. New Fuel Blocks are always increased by one, it cants jump numbers");
            }
        } else {
            // case where block is already commited on contract side but it is not finalized
            // on fuel side, maybe we didnt get enought consensus votes or there are network problems.
            // either way if this happens save it inside pending queue, it is maybe just a lag.

            // find block place and insert it. It iterates from the front to the back
            for pending in self.pending_blocks.iter_mut() {
                if pending.block_height == block.header.height {
                    pending.fuel_block = Some(block);
                    //TODO for missmatch of hash
                    break;
                }
            }
        }
    }

    /// Handle commited block from contract.
    pub fn block_commited(
        &mut self,
        block_root: Bytes32,
        height: BlockHeight,
        da_height: BlockHeight,
    ) {
        if self.pending_blocks.is_empty() {
            // no pending blocks, add one
            self.pending_blocks
                .push_front(PendingBlock::new_commited_block(
                    da_height, height, block_root,
                ));
            return;
        }

        let heightu64 = u64::from(height);
        let front_height = u64::from(self.pending_blocks.front().unwrap().block_height);
        let back_height = u64::from(self.pending_blocks.back().unwrap().block_height);

        if heightu64 < back_height {
            // ignore block that are not inside pending queue.
            panic!("Commited block is lower height then current lowest pending block.");
        } else if heightu64 > front_height {
            // fuel consensu is lagging and we received block commit before fuel block
            // push it into pending_block
            if heightu64 == front_height + 1 {
                self.pending_blocks
                    .push_front(PendingBlock::new_commited_block(
                        da_height, height, block_root,
                    ));
                return;
            } else {
                panic!("Something unexpected happened. New Fuel commits are always increased by one, it cants jump numbers ")
            }
        } else {
            // happy path. iterate over pending blocks and set it as commited.
            for pending in self.pending_blocks.iter_mut() {
                if pending.block_height == height {
                    pending.da_height = Some(da_height);
                    pending.reverted = false;
                    pending.block_height = height;
                    pending.block_root = block_root;
                    break;
                }
            }
        }
    }

    /// Handle revert of block commit from contract.
    pub fn block_reverted(
        &mut self,
        block_root: Bytes32,
        height: BlockHeight,
        da_height: BlockHeight,
    ) {
        // re-add block from bundle to be send to contract
        // handle commited block from contract, remove it from bundle
        if self.pending_blocks.is_empty() {
            // nothing to revert
            return;
        }

        let heightu64 = u64::from(height);
        let front_height = u64::from(self.pending_blocks.front().unwrap().block_height);
        let back_height = u64::from(self.pending_blocks.back().unwrap().block_height);

        if heightu64 < back_height {
            // ignore block that are not inside pending queue.
        } else if heightu64 > front_height {
            error!("Something unexpected happened.Reverted block commits are not something found in the future.");
        } else {
            // happy path. iterate over pending blocks and set it as commited.
            for pending in self.pending_blocks.iter_mut() {
                if pending.block_height > height {
                    pending.da_height = Some(da_height);
                    pending.reverted = true;
                    pending.block_height = height;
                    pending.block_root = block_root;
                }
            }
        }
    }

    /// When new block is created by this client, bundle all not commited blocks and send it to contract.
    pub async fn created_block<P>(&mut self, block: Arc<SealedFuelBlock>, provider: &Arc<P>)
    where
        P: Middleware + 'static,
    {
        self.new_fuel_block(block.clone());
        // bundle all blocks and send it to contract
        // assume that this is current greatest block and do bundle on it.
        // Assumption is made that we as elected leader have all previous blocks and have ability to
        // create new block in chain.
        let mut bundle = Vec::new();
        let mut iter = self.pending_blocks.iter_mut();
        while let Some(pending) = iter.next() {
            if pending.da_height.is_some() && pending.reverted == false {
                // we can assume that all blocks are there as we were not be able to
                // become leader of the round
                if let Some(pending) = pending.fuel_block.as_ref() {
                    bundle.push(pending.clone());
                } else {
                    error!("All fuel blocks should be present as we are in leader round.");
                    return;
                }
            } else {
                break;
            }
        }
        // we need one more parent that is already commited
        bundle.push(if let Some(last_parent) = iter.next() {
            if let Some(pending) = last_parent.fuel_block.as_ref() {
                pending.clone()
            } else {
                // TODO use last_finalized_block_commit if there is no last block inside pendings.
                // this can happen if there is there is stop and no new block are commited in last finalization period.
                let _ = self.last_finalized_block_commit;
                return;
            }
        } else {
            //todo!()
            // test
            error!("We dont have parent block to send it to block commit");
            return
        });

        bundle.reverse();

        let _ = self
            .commit_fuel_block(block.as_ref(), block.as_ref(), provider)
            .await;

        // TODO implement bundle send for testing this can be one by one send.
    }

    pub async fn current_block_commit<P>(
        &self,
        provider: &'static P,
    ) -> Result<(BlockHeight, Bytes32), anyhow::Error>
    where
        P: Middleware,
    {
        let provider = Arc::new(provider);
        let contract = ContractAbi::new(self.contract_address, provider);

        let block_hash = contract
            .s_current_block_id()
            .call()
            .await
            .map_err(anyhow::Error::msg)?;

        // TODO get current block commit

        Ok((BlockHeight::from(0u64), Bytes32::try_from(block_hash)?))
    }

    pub async fn commit_fuel_block<P>(
        &self,
        block: &SealedFuelBlock,
        parent: &SealedFuelBlock,
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
        let gas_oracle = EthGasStation::new(None);
        let provider = GasOracleMiddleware::new(provider, gas_oracle);

        // Manage nonces locally
        let provider = NonceManagerMiddleware::new(provider, address);

        // craft the tx
        let tx = TransactionRequest::new()
            .from(address)
            .to(self.contract_address)
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
