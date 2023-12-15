use crate::codec;
use crate::trigger::TransactionWithBlockPtr;
use graph::runtime::gas::GasCounter;
use graph::runtime::{asc_new, AscHeap, AscPtr, HostExportError, ToAscObj};
use graph_runtime_wasm::asc_abi::class::{Array, Uint8Array};

pub(crate) use super::generated::*;

impl ToAscObj<AscTag> for codec::Tag {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscTag, HostExportError> {
        Ok(AscTag {
            name: asc_new(heap, self.name.as_slice(), gas)?,
            value: asc_new(heap, self.value.as_slice(), gas)?,
        })
    }
}

impl ToAscObj<AscTransactionArray> for Vec<Vec<u8>> {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscTransactionArray, HostExportError> {
        let content = self
            .iter()
            .map(|x| asc_new(heap, x.as_slice(), gas))
            .collect::<Result<Vec<AscPtr<Uint8Array>>, _>>()?;
        Ok(AscTransactionArray(Array::new(&content, heap, gas)?))
    }
}

impl ToAscObj<AscTagArray> for Vec<codec::Tag> {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscTagArray, HostExportError> {
        let content = self
            .iter()
            .map(|x| asc_new(heap, x, gas))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AscTagArray(Array::new(&content, heap, gas)?))
    }
}

impl ToAscObj<AscProofOfAccess> for codec::ProofOfAccess {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscProofOfAccess, HostExportError> {
        Ok(AscProofOfAccess {
            option: asc_new(heap, &self.option, gas)?,
            tx_path: asc_new(heap, self.tx_path.as_slice(), gas)?,
            data_path: asc_new(heap, self.data_path.as_slice(), gas)?,
            chunk: asc_new(heap, self.chunk.as_slice(), gas)?,
        })
    }
}

impl ToAscObj<AscTransaction> for codec::Transaction {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscTransaction, HostExportError> {
        Ok(AscTransaction {
            format: self.format,
            id: asc_new(heap, self.id.as_slice(), gas)?,
            last_tx: asc_new(heap, self.last_tx.as_slice(), gas)?,
            owner: asc_new(heap, self.owner.as_slice(), gas)?,
            tags: asc_new(heap, &self.tags, gas)?,
            target: asc_new(heap, self.target.as_slice(), gas)?,
            quantity: asc_new(
                heap,
                self.quantity
                    .as_ref()
                    .map(|b| b.as_ref())
                    .unwrap_or_default(),
                gas,
            )?,
            data: asc_new(heap, self.data.as_slice(), gas)?,
            data_size: asc_new(
                heap,
                self.data_size
                    .as_ref()
                    .map(|b| b.as_ref())
                    .unwrap_or_default(),
                gas,
            )?,
            data_root: asc_new(heap, self.data_root.as_slice(), gas)?,
            signature: asc_new(heap, self.signature.as_slice(), gas)?,
            reward: asc_new(
                heap,
                self.reward.as_ref().map(|b| b.as_ref()).unwrap_or_default(),
                gas,
            )?,
        })
    }
}

impl ToAscObj<AscBlock> for codec::Block {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscBlock, HostExportError> {
        Ok(AscBlock {
            indep_hash: asc_new(heap, self.indep_hash.as_slice(), gas)?,
            nonce: asc_new(heap, self.nonce.as_slice(), gas)?,
            previous_block: asc_new(heap, self.previous_block.as_slice(), gas)?,
            timestamp: self.timestamp,
            last_retarget: self.last_retarget,
            diff: asc_new(
                heap,
                self.diff.as_ref().map(|b| b.as_ref()).unwrap_or_default(),
                gas,
            )?,
            height: self.height,
            hash: asc_new(heap, self.hash.as_slice(), gas)?,
            tx_root: asc_new(heap, self.tx_root.as_slice(), gas)?,
            txs: asc_new(
                heap,
                &self
                    .txs
                    .iter()
                    .map(|tx| tx.id.clone())
                    .collect::<Vec<Vec<u8>>>(),
                gas,
            )?,
            wallet_list: asc_new(heap, self.wallet_list.as_slice(), gas)?,
            reward_addr: asc_new(heap, self.reward_addr.as_slice(), gas)?,
            tags: asc_new(heap, &self.tags, gas)?,
            reward_pool: asc_new(
                heap,
                self.reward_pool
                    .as_ref()
                    .map(|b| b.as_ref())
                    .unwrap_or_default(),
                gas,
            )?,
            weave_size: asc_new(
                heap,
                self.weave_size
                    .as_ref()
                    .map(|b| b.as_ref())
                    .unwrap_or_default(),
                gas,
            )?,
            block_size: asc_new(
                heap,
                self.block_size
                    .as_ref()
                    .map(|b| b.as_ref())
                    .unwrap_or_default(),
                gas,
            )?,
            cumulative_diff: asc_new(
                heap,
                self.cumulative_diff
                    .as_ref()
                    .map(|b| b.as_ref())
                    .unwrap_or_default(),
                gas,
            )?,
            hash_list_merkle: asc_new(heap, self.hash_list_merkle.as_slice(), gas)?,
            poa: self
                .poa
                .as_ref()
                .map(|poa| asc_new(heap, poa, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
        })
    }
}

impl ToAscObj<AscTransactionWithBlockPtr> for TransactionWithBlockPtr {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscTransactionWithBlockPtr, HostExportError> {
        Ok(AscTransactionWithBlockPtr {
            tx: asc_new(heap, &self.tx.as_ref(), gas)?,
            block: asc_new(heap, self.block.as_ref(), gas)?,
        })
    }
}
