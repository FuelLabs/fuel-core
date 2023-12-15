#[rustfmt::skip]
#[path = "protobuf/sf.ethereum.r#type.v2.rs"]
mod pbcodec;

use anyhow::format_err;
use graph::{
    blockchain::{Block as BlockchainBlock, BlockPtr, ChainStoreBlock, ChainStoreData},
    prelude::{
        web3,
        web3::types::{Bytes, H160, H2048, H256, H64, U256, U64},
        BlockNumber, Error, EthereumBlock, EthereumBlockWithCalls, EthereumCall,
        LightEthereumBlock,
    },
};
use std::sync::Arc;
use std::{convert::TryFrom, fmt::Debug};

use crate::chain::BlockFinality;

pub use pbcodec::*;

trait TryDecodeProto<U, V>: Sized
where
    U: TryFrom<Self>,
    <U as TryFrom<Self>>::Error: Debug,
    V: From<U>,
{
    fn try_decode_proto(self, label: &'static str) -> Result<V, Error> {
        let u = U::try_from(self).map_err(|e| format_err!("invalid {}: {:?}", label, e))?;
        let v = V::from(u);
        Ok(v)
    }
}

impl TryDecodeProto<[u8; 256], H2048> for &[u8] {}
impl TryDecodeProto<[u8; 32], H256> for &[u8] {}
impl TryDecodeProto<[u8; 20], H160> for &[u8] {}

impl From<&BigInt> for web3::types::U256 {
    fn from(val: &BigInt) -> Self {
        web3::types::U256::from_big_endian(&val.bytes)
    }
}

pub struct CallAt<'a> {
    call: &'a Call,
    block: &'a Block,
    trace: &'a TransactionTrace,
}

impl<'a> CallAt<'a> {
    pub fn new(call: &'a Call, block: &'a Block, trace: &'a TransactionTrace) -> Self {
        Self { call, block, trace }
    }
}

impl<'a> TryInto<EthereumCall> for CallAt<'a> {
    type Error = Error;

    fn try_into(self) -> Result<EthereumCall, Self::Error> {
        Ok(EthereumCall {
            from: self.call.caller.try_decode_proto("call from address")?,
            to: self.call.address.try_decode_proto("call to address")?,
            value: self
                .call
                .value
                .as_ref()
                .map_or_else(|| U256::from(0), |v| v.into()),
            gas_used: U256::from(self.call.gas_consumed),
            input: Bytes(self.call.input.clone()),
            output: Bytes(self.call.return_data.clone()),
            block_hash: self.block.hash.try_decode_proto("call block hash")?,
            block_number: self.block.number as i32,
            transaction_hash: Some(self.trace.hash.try_decode_proto("call transaction hash")?),
            transaction_index: self.trace.index as u64,
        })
    }
}

impl TryInto<web3::types::Call> for Call {
    type Error = Error;

    fn try_into(self) -> Result<web3::types::Call, Self::Error> {
        Ok(web3::types::Call {
            from: self.caller.try_decode_proto("call from address")?,
            to: self.address.try_decode_proto("call to address")?,
            value: self
                .value
                .as_ref()
                .map_or_else(|| U256::from(0), |v| v.into()),
            gas: U256::from(self.gas_limit),
            input: Bytes::from(self.input.clone()),
            call_type: CallType::from_i32(self.call_type)
                .ok_or_else(|| format_err!("invalid call type: {}", self.call_type,))?
                .into(),
        })
    }
}

impl From<CallType> for web3::types::CallType {
    fn from(val: CallType) -> Self {
        match val {
            CallType::Unspecified => web3::types::CallType::None,
            CallType::Call => web3::types::CallType::Call,
            CallType::Callcode => web3::types::CallType::CallCode,
            CallType::Delegate => web3::types::CallType::DelegateCall,
            CallType::Static => web3::types::CallType::StaticCall,

            // FIXME (SF): Really not sure what this should map to, we are using None for now, need to revisit
            CallType::Create => web3::types::CallType::None,
        }
    }
}

pub struct LogAt<'a> {
    log: &'a Log,
    block: &'a Block,
    trace: &'a TransactionTrace,
}

impl<'a> LogAt<'a> {
    pub fn new(log: &'a Log, block: &'a Block, trace: &'a TransactionTrace) -> Self {
        Self { log, block, trace }
    }
}

impl<'a> TryInto<web3::types::Log> for LogAt<'a> {
    type Error = Error;

    fn try_into(self) -> Result<web3::types::Log, Self::Error> {
        Ok(web3::types::Log {
            address: self.log.address.try_decode_proto("log address")?,
            topics: self
                .log
                .topics
                .iter()
                .map(|t| t.try_decode_proto("topic"))
                .collect::<Result<Vec<H256>, Error>>()?,
            data: Bytes::from(self.log.data.clone()),
            block_hash: Some(self.block.hash.try_decode_proto("log block hash")?),
            block_number: Some(U64::from(self.block.number)),
            transaction_hash: Some(self.trace.hash.try_decode_proto("log transaction hash")?),
            transaction_index: Some(U64::from(self.trace.index as u64)),
            log_index: Some(U256::from(self.log.block_index)),
            transaction_log_index: Some(U256::from(self.log.index)),
            log_type: None,
            removed: None,
        })
    }
}

impl From<TransactionTraceStatus> for web3::types::U64 {
    fn from(val: TransactionTraceStatus) -> Self {
        let status: Option<web3::types::U64> = val.into();
        status.unwrap_or_else(|| web3::types::U64::from(0))
    }
}

impl Into<Option<web3::types::U64>> for TransactionTraceStatus {
    fn into(self) -> Option<web3::types::U64> {
        match self {
            Self::Unknown => {
                panic!("Got a transaction trace with status UNKNOWN, datasource is broken")
            }
            Self::Succeeded => Some(web3::types::U64::from(1)),
            Self::Failed => Some(web3::types::U64::from(0)),
            Self::Reverted => Some(web3::types::U64::from(0)),
        }
    }
}

pub struct TransactionTraceAt<'a> {
    trace: &'a TransactionTrace,
    block: &'a Block,
}

impl<'a> TransactionTraceAt<'a> {
    pub fn new(trace: &'a TransactionTrace, block: &'a Block) -> Self {
        Self { trace, block }
    }
}

impl<'a> TryInto<web3::types::Transaction> for TransactionTraceAt<'a> {
    type Error = Error;

    fn try_into(self) -> Result<web3::types::Transaction, Self::Error> {
        Ok(web3::types::Transaction {
            hash: self.trace.hash.try_decode_proto("transaction hash")?,
            nonce: U256::from(self.trace.nonce),
            block_hash: Some(self.block.hash.try_decode_proto("transaction block hash")?),
            block_number: Some(U64::from(self.block.number)),
            transaction_index: Some(U64::from(self.trace.index as u64)),
            from: Some(
                self.trace
                    .from
                    .try_decode_proto("transaction from address")?,
            ),
            to: match self.trace.calls.len() {
                0 => Some(self.trace.to.try_decode_proto("transaction to address")?),
                _ => {
                    match CallType::from_i32(self.trace.calls[0].call_type).ok_or_else(|| {
                        format_err!("invalid call type: {}", self.trace.calls[0].call_type,)
                    })? {
                        CallType::Create => {
                            None // we don't want the 'to' address on a transaction that creates the contract, to align with RPC behavior
                        }
                        _ => Some(self.trace.to.try_decode_proto("transaction to")?),
                    }
                }
            },
            value: self.trace.value.as_ref().map_or(U256::zero(), |x| x.into()),
            gas_price: self.trace.gas_price.as_ref().map(|x| x.into()),
            gas: U256::from(self.trace.gas_limit),
            input: Bytes::from(self.trace.input.clone()),
            v: None,
            r: None,
            s: None,
            raw: None,
            access_list: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            transaction_type: None,
        })
    }
}

impl TryInto<BlockFinality> for &Block {
    type Error = Error;

    fn try_into(self) -> Result<BlockFinality, Self::Error> {
        Ok(BlockFinality::NonFinal(self.try_into()?))
    }
}

impl TryInto<EthereumBlockWithCalls> for &Block {
    type Error = Error;

    fn try_into(self) -> Result<EthereumBlockWithCalls, Self::Error> {
        let header = self
            .header
            .as_ref()
            .expect("block header should always be present from gRPC Firehose");

        let block = EthereumBlockWithCalls {
            ethereum_block: EthereumBlock {
                block: Arc::new(LightEthereumBlock {
                    hash: Some(self.hash.try_decode_proto("block hash")?),
                    number: Some(U64::from(self.number)),
                    author: header.coinbase.try_decode_proto("author / coinbase")?,
                    parent_hash: header.parent_hash.try_decode_proto("parent hash")?,
                    uncles_hash: header.uncle_hash.try_decode_proto("uncle hash")?,
                    state_root: header.state_root.try_decode_proto("state root")?,
                    transactions_root: header
                        .transactions_root
                        .try_decode_proto("transactions root")?,
                    receipts_root: header.receipt_root.try_decode_proto("receipt root")?,
                    gas_used: U256::from(header.gas_used),
                    gas_limit: U256::from(header.gas_limit),
                    base_fee_per_gas: Some(
                        header
                            .base_fee_per_gas
                            .as_ref()
                            .map_or_else(U256::default, |v| v.into()),
                    ),
                    extra_data: Bytes::from(header.extra_data.clone()),
                    logs_bloom: match &header.logs_bloom.len() {
                        0 => None,
                        _ => Some(header.logs_bloom.try_decode_proto("logs bloom")?),
                    },
                    timestamp: header
                        .timestamp
                        .as_ref()
                        .map_or_else(U256::default, |v| U256::from(v.seconds)),
                    difficulty: header
                        .difficulty
                        .as_ref()
                        .map_or_else(U256::default, |v| v.into()),
                    total_difficulty: Some(
                        header
                            .total_difficulty
                            .as_ref()
                            .map_or_else(U256::default, |v| v.into()),
                    ),
                    // FIXME (SF): Firehose does not have seal fields, are they really used? Might be required for POA chains only also, I've seen that stuff on xDai (is this important?)
                    seal_fields: vec![],
                    uncles: self
                        .uncles
                        .iter()
                        .map(|u| u.hash.try_decode_proto("uncle hash"))
                        .collect::<Result<Vec<H256>, _>>()?,
                    transactions: self
                        .transaction_traces
                        .iter()
                        .map(|t| TransactionTraceAt::new(t, self).try_into())
                        .collect::<Result<Vec<web3::types::Transaction>, Error>>()?,
                    size: Some(U256::from(self.size)),
                    mix_hash: Some(header.mix_hash.try_decode_proto("mix hash")?),
                    nonce: Some(H64::from_low_u64_be(header.nonce)),
                }),
                transaction_receipts: self
                    .transaction_traces
                    .iter()
                    .filter_map(|t| {
                        t.receipt.as_ref().map(|r| {
                            Ok(web3::types::TransactionReceipt {
                                transaction_hash: t.hash.try_decode_proto("transaction hash")?,
                                transaction_index: U64::from(t.index),
                                block_hash: Some(
                                    self.hash.try_decode_proto("transaction block hash")?,
                                ),
                                block_number: Some(U64::from(self.number)),
                                cumulative_gas_used: U256::from(r.cumulative_gas_used),
                                // FIXME (SF): What is the rule here about gas_used being None, when it's 0?
                                gas_used: Some(U256::from(t.gas_used)),
                                contract_address: {
                                    match t.calls.len() {
                                        0 => None,
                                        _ => {
                                            match CallType::from_i32(t.calls[0].call_type)
                                                .ok_or_else(|| {
                                                    format_err!(
                                                        "invalid call type: {}",
                                                        t.calls[0].call_type,
                                                    )
                                                })? {
                                                CallType::Create => {
                                                    Some(t.calls[0].address.try_decode_proto(
                                                        "transaction contract address",
                                                    )?)
                                                }
                                                _ => None,
                                            }
                                        }
                                    }
                                },
                                logs: r
                                    .logs
                                    .iter()
                                    .map(|l| LogAt::new(l, self, t).try_into())
                                    .collect::<Result<Vec<_>, Error>>()?,
                                status: TransactionTraceStatus::from_i32(t.status)
                                    .ok_or_else(|| {
                                        format_err!(
                                            "invalid transaction trace status: {}",
                                            t.status
                                        )
                                    })?
                                    .into(),
                                root: match r.state_root.len() {
                                    0 => None, // FIXME (SF): should this instead map to [0;32]?
                                    // FIXME (SF): if len < 32, what do we do?
                                    _ => Some(
                                        r.state_root.try_decode_proto("transaction state root")?,
                                    ),
                                },
                                logs_bloom: r
                                    .logs_bloom
                                    .try_decode_proto("transaction logs bloom")?,
                                from: t.from.try_decode_proto("transaction from")?,
                                to: Some(t.to.try_decode_proto("transaction to")?),
                                transaction_type: None,
                                effective_gas_price: None,
                            })
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?
                    .into_iter()
                    // Transaction receipts will be shared along the code, so we put them into an
                    // Arc here to avoid excessive cloning.
                    .map(Arc::new)
                    .collect(),
            },
            // Comment (437a9f17-67cc-478f-80a3-804fe554b227): This Some() will avoid calls in the triggers_in_block
            // TODO: Refactor in a way that this is no longer needed.
            calls: Some(
                self.transaction_traces
                    .iter()
                    .flat_map(|trace| {
                        trace
                            .calls
                            .iter()
                            .filter(|call| !call.status_reverted && !call.status_failed)
                            .map(|call| CallAt::new(call, self, trace).try_into())
                            .collect::<Vec<Result<EthereumCall, Error>>>()
                    })
                    .collect::<Result<_, _>>()?,
            ),
        };

        Ok(block)
    }
}

impl BlockHeader {
    pub fn parent_ptr(&self) -> Option<BlockPtr> {
        match self.parent_hash.len() {
            0 => None,
            _ => Some(BlockPtr::from((
                H256::from_slice(self.parent_hash.as_ref()),
                self.number - 1,
            ))),
        }
    }
}

impl<'a> From<&'a BlockHeader> for BlockPtr {
    fn from(b: &'a BlockHeader) -> BlockPtr {
        BlockPtr::from((H256::from_slice(b.hash.as_ref()), b.number))
    }
}

impl<'a> From<&'a Block> for BlockPtr {
    fn from(b: &'a Block) -> BlockPtr {
        BlockPtr::from((H256::from_slice(b.hash.as_ref()), b.number))
    }
}

impl Block {
    pub fn header(&self) -> &BlockHeader {
        self.header.as_ref().unwrap()
    }

    pub fn ptr(&self) -> BlockPtr {
        BlockPtr::from(self.header())
    }

    pub fn parent_ptr(&self) -> Option<BlockPtr> {
        self.header().parent_ptr()
    }
}

impl BlockchainBlock for Block {
    fn number(&self) -> i32 {
        BlockNumber::try_from(self.header().number).unwrap()
    }

    fn ptr(&self) -> BlockPtr {
        self.into()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        self.parent_ptr()
    }

    // This implementation provides the timestamp so that it works with block _meta's timestamp.
    // However, the firehose types will not populate the transaction receipts so switching back
    // from firehose ingestor to the firehose ingestor will prevent non final block from being
    // processed using the block stored by firehose.
    fn data(&self) -> Result<jsonrpc_core::serde_json::Value, jsonrpc_core::serde_json::Error> {
        self.header().to_json()
    }
}

impl HeaderOnlyBlock {
    pub fn header(&self) -> &BlockHeader {
        self.header.as_ref().unwrap()
    }
}

impl From<&BlockHeader> for ChainStoreData {
    fn from(val: &BlockHeader) -> Self {
        ChainStoreData {
            block: ChainStoreBlock::new(
                val.timestamp.as_ref().unwrap().seconds,
                jsonrpc_core::Value::Null,
            ),
        }
    }
}

impl BlockHeader {
    fn to_json(&self) -> Result<jsonrpc_core::serde_json::Value, jsonrpc_core::serde_json::Error> {
        let chain_store_data: ChainStoreData = self.into();

        jsonrpc_core::to_value(chain_store_data)
    }
}

impl<'a> From<&'a HeaderOnlyBlock> for BlockPtr {
    fn from(b: &'a HeaderOnlyBlock) -> BlockPtr {
        BlockPtr::from(b.header())
    }
}

impl BlockchainBlock for HeaderOnlyBlock {
    fn number(&self) -> i32 {
        BlockNumber::try_from(self.header().number).unwrap()
    }

    fn ptr(&self) -> BlockPtr {
        self.into()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        self.header().parent_ptr()
    }

    // This implementation provides the timestamp so that it works with block _meta's timestamp.
    // However, the firehose types will not populate the transaction receipts so switching back
    // from firehose ingestor to the firehose ingestor will prevent non final block from being
    // processed using the block stored by firehose.
    fn data(&self) -> Result<jsonrpc_core::serde_json::Value, jsonrpc_core::serde_json::Error> {
        self.header().to_json()
    }
}

#[cfg(test)]
mod test {
    use graph::{blockchain::Block as _, prelude::chrono::Utc};
    use prost_types::Timestamp;

    use crate::codec::BlockHeader;

    use super::Block;

    #[test]
    fn ensure_block_serialization() {
        let now = Utc::now().timestamp();
        let mut block = Block::default();
        let mut header = BlockHeader::default();
        header.timestamp = Some(Timestamp {
            seconds: now,
            nanos: 0,
        });

        block.header = Some(header);

        let str_block = block.data().unwrap().to_string();

        assert_eq!(
            str_block,
            // if you're confused when reading this, format needs {{ to escape {
            format!(r#"{{"block":{{"data":null,"timestamp":"{}"}}}}"#, now)
        );
    }
}
