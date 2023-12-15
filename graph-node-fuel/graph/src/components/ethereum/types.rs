use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, sync::Arc};
use web3::types::{
    Action, Address, Block, Bytes, Log, Res, Trace, Transaction, TransactionReceipt, H256, U256,
    U64,
};

use crate::{blockchain::BlockPtr, prelude::BlockNumber};

pub type LightEthereumBlock = Block<Transaction>;

pub trait LightEthereumBlockExt {
    fn number(&self) -> BlockNumber;
    fn transaction_for_log(&self, log: &Log) -> Option<Transaction>;
    fn transaction_for_call(&self, call: &EthereumCall) -> Option<Transaction>;
    fn parent_ptr(&self) -> Option<BlockPtr>;
    fn format(&self) -> String;
    fn block_ptr(&self) -> BlockPtr;
}

impl LightEthereumBlockExt for LightEthereumBlock {
    fn number(&self) -> BlockNumber {
        BlockNumber::try_from(self.number.unwrap().as_u64()).unwrap()
    }

    fn transaction_for_log(&self, log: &Log) -> Option<Transaction> {
        log.transaction_hash
            .and_then(|hash| self.transactions.iter().find(|tx| tx.hash == hash))
            .cloned()
    }

    fn transaction_for_call(&self, call: &EthereumCall) -> Option<Transaction> {
        call.transaction_hash
            .and_then(|hash| self.transactions.iter().find(|tx| tx.hash == hash))
            .cloned()
    }

    fn parent_ptr(&self) -> Option<BlockPtr> {
        match self.number() {
            0 => None,
            n => Some(BlockPtr::from((self.parent_hash, n - 1))),
        }
    }

    fn format(&self) -> String {
        format!(
            "{} ({})",
            self.number
                .map_or(String::from("none"), |number| format!("#{}", number)),
            self.hash
                .map_or(String::from("-"), |hash| format!("{:x}", hash))
        )
    }

    fn block_ptr(&self) -> BlockPtr {
        BlockPtr::from((self.hash.unwrap(), self.number.unwrap().as_u64()))
    }
}

#[derive(Clone, Debug)]
pub struct EthereumBlockWithCalls {
    pub ethereum_block: EthereumBlock,
    /// The calls in this block; `None` means we haven't checked yet,
    /// `Some(vec![])` means that we checked and there were none
    pub calls: Option<Vec<EthereumCall>>,
}

impl EthereumBlockWithCalls {
    /// Given an `EthereumCall`, check within receipts if that transaction was successful.
    pub fn transaction_for_call_succeeded(&self, call: &EthereumCall) -> anyhow::Result<bool> {
        let call_transaction_hash = call.transaction_hash.ok_or(anyhow::anyhow!(
            "failed to find a transaction for this call"
        ))?;

        let receipt = self
            .ethereum_block
            .transaction_receipts
            .iter()
            .find(|txn| txn.transaction_hash == call_transaction_hash)
            .ok_or(anyhow::anyhow!(
                "failed to find the receipt for this transaction"
            ))?;

        Ok(evaluate_transaction_status(receipt.status))
    }
}

/// Evaluates if a given transaction was successful.
///
/// Returns `true` on success and `false` on failure.
/// If a receipt does not have a status value (EIP-658), assume the transaction was successful.
pub fn evaluate_transaction_status(receipt_status: Option<U64>) -> bool {
    receipt_status
        .map(|status| !status.is_zero())
        .unwrap_or(true)
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct EthereumBlock {
    pub block: Arc<LightEthereumBlock>,
    pub transaction_receipts: Vec<Arc<TransactionReceipt>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EthereumCall {
    pub from: Address,
    pub to: Address,
    pub value: U256,
    pub gas_used: U256,
    pub input: Bytes,
    pub output: Bytes,
    pub block_number: BlockNumber,
    pub block_hash: H256,
    pub transaction_hash: Option<H256>,
    pub transaction_index: u64,
}

impl EthereumCall {
    pub fn try_from_trace(trace: &Trace) -> Option<Self> {
        // The parity-ethereum tracing api returns traces for operations which had execution errors.
        // Filter errorful traces out, since call handlers should only run on successful CALLs.
        if trace.error.is_some() {
            return None;
        }
        // We are only interested in traces from CALLs
        let call = match &trace.action {
            // Contract to contract value transfers compile to the CALL opcode
            // and have no input. Call handlers are for triggering on explicit method calls right now.
            Action::Call(call) if call.input.0.len() >= 4 => call,
            _ => return None,
        };
        let (output, gas_used) = match &trace.result {
            Some(Res::Call(result)) => (result.output.clone(), result.gas_used),
            _ => return None,
        };

        // The only traces without transactions are those from Parity block reward contracts, we
        // don't support triggering on that.
        let transaction_index = trace.transaction_position? as u64;

        Some(EthereumCall {
            from: call.from,
            to: call.to,
            value: call.value,
            gas_used,
            input: call.input.clone(),
            output,
            block_number: trace.block_number as BlockNumber,
            block_hash: trace.block_hash,
            transaction_hash: trace.transaction_hash,
            transaction_index,
        })
    }
}

impl From<EthereumBlock> for BlockPtr {
    fn from(b: EthereumBlock) -> BlockPtr {
        BlockPtr::from((b.block.hash.unwrap(), b.block.number.unwrap().as_u64()))
    }
}

impl<'a> From<&'a EthereumBlock> for BlockPtr {
    fn from(b: &'a EthereumBlock) -> BlockPtr {
        BlockPtr::from((b.block.hash.unwrap(), b.block.number.unwrap().as_u64()))
    }
}

impl<'a> From<&'a EthereumCall> for BlockPtr {
    fn from(call: &'a EthereumCall) -> BlockPtr {
        BlockPtr::from((call.block_hash, call.block_number))
    }
}
