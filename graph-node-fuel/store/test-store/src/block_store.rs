use std::{convert::TryFrom, str::FromStr, sync::Arc};

use lazy_static::lazy_static;

use graph::components::store::BlockStore;
use graph::{
    blockchain::Block as BlockchainBlock,
    prelude::{
        serde_json, web3::types::H256, web3::types::U256, BlockHash, BlockNumber, BlockPtr,
        EthereumBlock, LightEthereumBlock,
    },
};
use graph_chain_ethereum::codec::{Block, BlockHeader};
use prost_types::Timestamp;

lazy_static! {
    // Genesis block
    pub static ref GENESIS_BLOCK: FakeBlock = FakeBlock {
        number: super::GENESIS_PTR.number,
        hash: super::GENESIS_PTR.hash_hex(),
        timestamp: None,
        parent_hash: NO_PARENT.to_string()
    };
    pub static ref BLOCK_ONE: FakeBlock = GENESIS_BLOCK
        .make_child("8511fa04b64657581e3f00e14543c1d522d5d7e771b54aa3060b662ade47da13", None);
    pub static ref BLOCK_ONE_SIBLING: FakeBlock =
        GENESIS_BLOCK.make_child("b98fb783b49de5652097a989414c767824dff7e7fd765a63b493772511db81c1", None);
    pub static ref BLOCK_ONE_NO_PARENT: FakeBlock = FakeBlock::make_no_parent(
        1,
        "7205bdfcf4521874cf38ce38c879ff967bf3a069941286bfe267109ad275a63d"
    );

    pub static ref BLOCK_TWO: FakeBlock = BLOCK_ONE.make_child("f8ccbd3877eb98c958614f395dd351211afb9abba187bfc1fb4ac414b099c4a6", None);
    pub static ref BLOCK_TWO_NO_PARENT: FakeBlock = FakeBlock::make_no_parent(2, "3b652b00bff5e168b1218ff47593d516123261c4487629c4175f642ee56113fe");
    pub static ref BLOCK_THREE: FakeBlock = BLOCK_TWO.make_child("7347afe69254df06729e123610b00b8b11f15cfae3241f9366fb113aec07489c", None);
    pub static ref BLOCK_THREE_NO_PARENT: FakeBlock = FakeBlock::make_no_parent(3, "fa9ebe3f74de4c56908b49f5c4044e85825f7350f3fa08a19151de82a82a7313");
    pub static ref BLOCK_THREE_TIMESTAMP: FakeBlock = BLOCK_TWO.make_child("6b834521bb753c132fdcf0e1034803ed9068e324112f8750ba93580b393a986b", Some(U256::from(1657712166)));
    pub static ref BLOCK_THREE_TIMESTAMP_FIREHOSE: FakeBlock = BLOCK_TWO.make_child("6b834521bb753c132fdcf0e1034803ed9068e324112f8750ba93580b393a986f", Some(U256::from(1657712166)));
    // This block is special and serializes in a slightly different way, this is needed to simulate non-ethereum behaviour at the store level. If you're not sure
    // what you are doing, don't use this block for other tests.
    pub static ref BLOCK_THREE_NO_TIMESTAMP: FakeBlock = BLOCK_TWO.make_child("6b834521bb753c132fdcf0e1034803ed9068e324112f8750ba93580b393a986b", None);
    pub static ref BLOCK_FOUR: FakeBlock = BLOCK_THREE.make_child("7cce080f5a49c2997a6cc65fc1cee9910fd8fc3721b7010c0b5d0873e2ac785e", None);
    pub static ref BLOCK_FIVE: FakeBlock = BLOCK_FOUR.make_child("7b0ea919e258eb2b119eb32de56b85d12d50ac6a9f7c5909f843d6172c8ba196", None);
    pub static ref BLOCK_SIX_NO_PARENT: FakeBlock = FakeBlock::make_no_parent(6, "6b834521bb753c132fdcf0e1034803ed9068e324112f8750ba93580b393a986b");
}

// Hash indicating 'no parent'
pub const NO_PARENT: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// The parts of an Ethereum block that are interesting for these tests:
/// the block number, hash, and the hash of the parent block
#[derive(Clone, Debug, PartialEq)]
pub struct FakeBlock {
    pub number: BlockNumber,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: Option<U256>,
}

impl FakeBlock {
    pub fn make_child(&self, hash: &str, timestamp: Option<U256>) -> Self {
        FakeBlock {
            number: self.number + 1,
            hash: hash.to_owned(),
            parent_hash: self.hash.clone(),
            timestamp,
        }
    }

    pub fn make_no_parent(number: BlockNumber, hash: &str) -> Self {
        FakeBlock {
            number,
            hash: hash.to_owned(),
            parent_hash: NO_PARENT.to_string(),
            timestamp: None,
        }
    }

    pub fn block_hash(&self) -> BlockHash {
        BlockHash::from_str(self.hash.as_str()).expect("invalid block hash")
    }

    pub fn block_ptr(&self) -> BlockPtr {
        BlockPtr::new(self.block_hash(), self.number)
    }

    pub fn as_ethereum_block(&self) -> EthereumBlock {
        let parent_hash = H256::from_str(self.parent_hash.as_str()).expect("invalid parent hash");

        let mut block = LightEthereumBlock::default();
        block.number = Some(self.number.into());
        block.parent_hash = parent_hash;
        block.hash = Some(H256(self.block_hash().as_slice().try_into().unwrap()));
        if let Some(ts) = self.timestamp {
            block.timestamp = ts;
        }

        EthereumBlock {
            block: Arc::new(block),
            transaction_receipts: Vec::new(),
        }
    }

    pub fn as_firehose_block(&self) -> Block {
        let mut block = Block::default();
        block.hash = self.hash.clone().into_bytes();
        block.number = self.number as u64;

        let mut header = BlockHeader::default();
        header.parent_hash = self.parent_hash.clone().into_bytes();
        header.timestamp = self.timestamp.map(|ts| Timestamp {
            seconds: i64::from_str_radix(&ts.to_string(), 10).unwrap(),
            nanos: 0,
        });
        block.header = Some(header);

        block
    }
}

impl BlockchainBlock for FakeBlock {
    fn ptr(&self) -> BlockPtr {
        self.block_ptr()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        if self.number > 0 {
            Some(
                BlockPtr::try_from((self.parent_hash.as_str(), (self.number - 1) as i64))
                    .expect("can construct parent ptr"),
            )
        } else {
            None
        }
    }

    fn data(&self) -> Result<serde_json::Value, serde_json::Error> {
        let mut value: serde_json::Value = if self.eq(&BLOCK_THREE_TIMESTAMP_FIREHOSE) {
            self.as_firehose_block().data().unwrap()
        } else {
            serde_json::to_value(self.as_ethereum_block())?
        };

        if !self.eq(&BLOCK_THREE_NO_TIMESTAMP) {
            return Ok(value);
        };

        // Remove the timestamp for block BLOCK_THREE_NO_TIMESTAMP in order to simulate the non EVM behaviour
        // In these cases timestamp is not there at all but LightEthereumBlock uses U256 as timestamp so it
        // can never be null and therefore impossible to test without manipulating the JSON blob directly.
        if let serde_json::Value::Object(ref mut map) = value {
            map.entry("block").and_modify(|ref mut block| {
                if let serde_json::Value::Object(ref mut block) = block {
                    block.remove_entry("timestamp");
                }
            });
        };

        Ok(value)
    }
}

pub type FakeBlockList = Vec<&'static FakeBlock>;

/// Store the given chain as the blocks for the `network` set the
/// network's genesis block to `genesis_hash`, and head block to
/// `null`
pub async fn set_chain(chain: FakeBlockList, network: &str) -> Vec<(BlockPtr, BlockHash)> {
    let store = crate::store::STORE
        .block_store()
        .chain_store(network)
        .unwrap();
    let chain: Vec<Arc<dyn BlockchainBlock>> = chain
        .iter()
        .cloned()
        .map(|block| Arc::new(block.clone()) as Arc<dyn BlockchainBlock>)
        .collect();
    store.set_chain(&GENESIS_BLOCK.hash, chain).await
}
