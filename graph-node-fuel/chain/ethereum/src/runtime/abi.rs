use super::runtime_adapter::UnresolvedContractCall;
use crate::trigger::{
    EthereumBlockData, EthereumCallData, EthereumEventData, EthereumTransactionData,
};
use graph::{
    prelude::{
        ethabi,
        web3::types::{Log, TransactionReceipt, H256},
        BigInt,
    },
    runtime::{
        asc_get, asc_new, gas::GasCounter, AscHeap, AscIndexId, AscPtr, AscType,
        DeterministicHostError, FromAscObj, HostExportError, IndexForAscTypeId, ToAscObj,
    },
};
use graph_runtime_derive::AscType;
use graph_runtime_wasm::asc_abi::class::{
    Array, AscAddress, AscBigInt, AscEnum, AscH160, AscString, AscWrapped, EthereumValueKind,
    Uint8Array,
};
use semver::Version;

type AscH256 = Uint8Array;
type AscH2048 = Uint8Array;

pub struct AscLogParamArray(Array<AscPtr<AscLogParam>>);

impl AscType for AscLogParamArray {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }
    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        Ok(Self(Array::from_asc_bytes(asc_obj, api_version)?))
    }
}

impl ToAscObj<AscLogParamArray> for Vec<ethabi::LogParam> {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscLogParamArray, HostExportError> {
        let content: Result<Vec<_>, _> = self.iter().map(|x| asc_new(heap, x, gas)).collect();
        let content = content?;
        Ok(AscLogParamArray(Array::new(&content, heap, gas)?))
    }
}

impl AscIndexId for AscLogParamArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayEventParam;
}

pub struct AscTopicArray(Array<AscPtr<AscH256>>);

impl AscType for AscTopicArray {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        Ok(Self(Array::from_asc_bytes(asc_obj, api_version)?))
    }
}

impl ToAscObj<AscTopicArray> for Vec<H256> {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscTopicArray, HostExportError> {
        let topics = self
            .iter()
            .map(|topic| asc_new(heap, topic, gas))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AscTopicArray(Array::new(&topics, heap, gas)?))
    }
}

impl AscIndexId for AscTopicArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayH256;
}

pub struct AscLogArray(Array<AscPtr<AscEthereumLog>>);

impl AscType for AscLogArray {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        Ok(Self(Array::from_asc_bytes(asc_obj, api_version)?))
    }
}

impl ToAscObj<AscLogArray> for Vec<Log> {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscLogArray, HostExportError> {
        let logs = self
            .iter()
            .map(|log| asc_new(heap, &log, gas))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AscLogArray(Array::new(&logs, heap, gas)?))
    }
}

impl AscIndexId for AscLogArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayLog;
}

#[repr(C)]
#[derive(AscType)]
pub struct AscUnresolvedContractCall_0_0_4 {
    pub contract_name: AscPtr<AscString>,
    pub contract_address: AscPtr<AscAddress>,
    pub function_name: AscPtr<AscString>,
    pub function_signature: AscPtr<AscString>,
    pub function_args: AscPtr<Array<AscPtr<AscEnum<EthereumValueKind>>>>,
}

impl AscIndexId for AscUnresolvedContractCall_0_0_4 {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::SmartContractCall;
}

impl FromAscObj<AscUnresolvedContractCall_0_0_4> for UnresolvedContractCall {
    fn from_asc_obj<H: AscHeap + ?Sized>(
        asc_call: AscUnresolvedContractCall_0_0_4,
        heap: &H,
        gas: &GasCounter,
        depth: usize,
    ) -> Result<Self, DeterministicHostError> {
        Ok(UnresolvedContractCall {
            contract_name: asc_get(heap, asc_call.contract_name, gas, depth)?,
            contract_address: asc_get(heap, asc_call.contract_address, gas, depth)?,
            function_name: asc_get(heap, asc_call.function_name, gas, depth)?,
            function_signature: Some(asc_get(heap, asc_call.function_signature, gas, depth)?),
            function_args: asc_get(heap, asc_call.function_args, gas, depth)?,
        })
    }
}

#[repr(C)]
#[derive(AscType)]
pub struct AscUnresolvedContractCall {
    pub contract_name: AscPtr<AscString>,
    pub contract_address: AscPtr<AscAddress>,
    pub function_name: AscPtr<AscString>,
    pub function_args: AscPtr<Array<AscPtr<AscEnum<EthereumValueKind>>>>,
}

impl FromAscObj<AscUnresolvedContractCall> for UnresolvedContractCall {
    fn from_asc_obj<H: AscHeap + ?Sized>(
        asc_call: AscUnresolvedContractCall,
        heap: &H,
        gas: &GasCounter,
        depth: usize,
    ) -> Result<Self, DeterministicHostError> {
        Ok(UnresolvedContractCall {
            contract_name: asc_get(heap, asc_call.contract_name, gas, depth)?,
            contract_address: asc_get(heap, asc_call.contract_address, gas, depth)?,
            function_name: asc_get(heap, asc_call.function_name, gas, depth)?,
            function_signature: None,
            function_args: asc_get(heap, asc_call.function_args, gas, depth)?,
        })
    }
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumBlock {
    pub hash: AscPtr<AscH256>,
    pub parent_hash: AscPtr<AscH256>,
    pub uncles_hash: AscPtr<AscH256>,
    pub author: AscPtr<AscH160>,
    pub state_root: AscPtr<AscH256>,
    pub transactions_root: AscPtr<AscH256>,
    pub receipts_root: AscPtr<AscH256>,
    pub number: AscPtr<AscBigInt>,
    pub gas_used: AscPtr<AscBigInt>,
    pub gas_limit: AscPtr<AscBigInt>,
    pub timestamp: AscPtr<AscBigInt>,
    pub difficulty: AscPtr<AscBigInt>,
    pub total_difficulty: AscPtr<AscBigInt>,
    pub size: AscPtr<AscBigInt>,
}

impl AscIndexId for AscEthereumBlock {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumBlock;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumBlock_0_0_6 {
    pub hash: AscPtr<AscH256>,
    pub parent_hash: AscPtr<AscH256>,
    pub uncles_hash: AscPtr<AscH256>,
    pub author: AscPtr<AscH160>,
    pub state_root: AscPtr<AscH256>,
    pub transactions_root: AscPtr<AscH256>,
    pub receipts_root: AscPtr<AscH256>,
    pub number: AscPtr<AscBigInt>,
    pub gas_used: AscPtr<AscBigInt>,
    pub gas_limit: AscPtr<AscBigInt>,
    pub timestamp: AscPtr<AscBigInt>,
    pub difficulty: AscPtr<AscBigInt>,
    pub total_difficulty: AscPtr<AscBigInt>,
    pub size: AscPtr<AscBigInt>,
    pub base_fee_per_block: AscPtr<AscBigInt>,
}

impl AscIndexId for AscEthereumBlock_0_0_6 {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumBlock;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumTransaction_0_0_1 {
    pub hash: AscPtr<AscH256>,
    pub index: AscPtr<AscBigInt>,
    pub from: AscPtr<AscH160>,
    pub to: AscPtr<AscH160>,
    pub value: AscPtr<AscBigInt>,
    pub gas_limit: AscPtr<AscBigInt>,
    pub gas_price: AscPtr<AscBigInt>,
}

impl AscIndexId for AscEthereumTransaction_0_0_1 {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumTransaction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumTransaction_0_0_2 {
    pub hash: AscPtr<AscH256>,
    pub index: AscPtr<AscBigInt>,
    pub from: AscPtr<AscH160>,
    pub to: AscPtr<AscH160>,
    pub value: AscPtr<AscBigInt>,
    pub gas_limit: AscPtr<AscBigInt>,
    pub gas_price: AscPtr<AscBigInt>,
    pub input: AscPtr<Uint8Array>,
}

impl AscIndexId for AscEthereumTransaction_0_0_2 {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumTransaction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumTransaction_0_0_6 {
    pub hash: AscPtr<AscH256>,
    pub index: AscPtr<AscBigInt>,
    pub from: AscPtr<AscH160>,
    pub to: AscPtr<AscH160>,
    pub value: AscPtr<AscBigInt>,
    pub gas_limit: AscPtr<AscBigInt>,
    pub gas_price: AscPtr<AscBigInt>,
    pub input: AscPtr<Uint8Array>,
    pub nonce: AscPtr<AscBigInt>,
}

impl AscIndexId for AscEthereumTransaction_0_0_6 {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumTransaction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumEvent<T, B>
where
    T: AscType,
    B: AscType,
{
    pub address: AscPtr<AscAddress>,
    pub log_index: AscPtr<AscBigInt>,
    pub transaction_log_index: AscPtr<AscBigInt>,
    pub log_type: AscPtr<AscString>,
    pub block: AscPtr<B>,
    pub transaction: AscPtr<T>,
    pub params: AscPtr<AscLogParamArray>,
}

impl AscIndexId for AscEthereumEvent<AscEthereumTransaction_0_0_1, AscEthereumBlock> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumEvent;
}

impl AscIndexId for AscEthereumEvent<AscEthereumTransaction_0_0_2, AscEthereumBlock> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumEvent;
}

impl AscIndexId for AscEthereumEvent<AscEthereumTransaction_0_0_6, AscEthereumBlock_0_0_6> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumEvent;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumLog {
    pub address: AscPtr<AscAddress>,
    pub topics: AscPtr<AscTopicArray>,
    pub data: AscPtr<Uint8Array>,
    pub block_hash: AscPtr<AscH256>,
    pub block_number: AscPtr<AscH256>,
    pub transaction_hash: AscPtr<AscH256>,
    pub transaction_index: AscPtr<AscBigInt>,
    pub log_index: AscPtr<AscBigInt>,
    pub transaction_log_index: AscPtr<AscBigInt>,
    pub log_type: AscPtr<AscString>,
    pub removed: AscPtr<AscWrapped<bool>>,
}

impl AscIndexId for AscEthereumLog {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Log;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumTransactionReceipt {
    pub transaction_hash: AscPtr<AscH256>,
    pub transaction_index: AscPtr<AscBigInt>,
    pub block_hash: AscPtr<AscH256>,
    pub block_number: AscPtr<AscBigInt>,
    pub cumulative_gas_used: AscPtr<AscBigInt>,
    pub gas_used: AscPtr<AscBigInt>,
    pub contract_address: AscPtr<AscAddress>,
    pub logs: AscPtr<AscLogArray>,
    pub status: AscPtr<AscBigInt>,
    pub root: AscPtr<AscH256>,
    pub logs_bloom: AscPtr<AscH2048>,
}

impl AscIndexId for AscEthereumTransactionReceipt {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::TransactionReceipt;
}

/// Introduced in API Version 0.0.7, this is the same as [`AscEthereumEvent`] with an added
/// `receipt` field.
#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumEvent_0_0_7<T, B>
where
    T: AscType,
    B: AscType,
{
    pub address: AscPtr<AscAddress>,
    pub log_index: AscPtr<AscBigInt>,
    pub transaction_log_index: AscPtr<AscBigInt>,
    pub log_type: AscPtr<AscString>,
    pub block: AscPtr<B>,
    pub transaction: AscPtr<T>,
    pub params: AscPtr<AscLogParamArray>,
    pub receipt: AscPtr<AscEthereumTransactionReceipt>,
}

impl AscIndexId for AscEthereumEvent_0_0_7<AscEthereumTransaction_0_0_6, AscEthereumBlock_0_0_6> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumEvent;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscLogParam {
    pub name: AscPtr<AscString>,
    pub value: AscPtr<AscEnum<EthereumValueKind>>,
}

impl AscIndexId for AscLogParam {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EventParam;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumCall {
    pub address: AscPtr<AscAddress>,
    pub block: AscPtr<AscEthereumBlock>,
    pub transaction: AscPtr<AscEthereumTransaction_0_0_1>,
    pub inputs: AscPtr<AscLogParamArray>,
    pub outputs: AscPtr<AscLogParamArray>,
}

impl AscIndexId for AscEthereumCall {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumCall;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumCall_0_0_3<T, B>
where
    T: AscType,
    B: AscType,
{
    pub to: AscPtr<AscAddress>,
    pub from: AscPtr<AscAddress>,
    pub block: AscPtr<B>,
    pub transaction: AscPtr<T>,
    pub inputs: AscPtr<AscLogParamArray>,
    pub outputs: AscPtr<AscLogParamArray>,
}

impl<T, B> AscIndexId for AscEthereumCall_0_0_3<T, B>
where
    T: AscType,
    B: AscType,
{
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumCall;
}

impl ToAscObj<AscEthereumBlock> for EthereumBlockData {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumBlock, HostExportError> {
        Ok(AscEthereumBlock {
            hash: asc_new(heap, &self.hash, gas)?,
            parent_hash: asc_new(heap, &self.parent_hash, gas)?,
            uncles_hash: asc_new(heap, &self.uncles_hash, gas)?,
            author: asc_new(heap, &self.author, gas)?,
            state_root: asc_new(heap, &self.state_root, gas)?,
            transactions_root: asc_new(heap, &self.transactions_root, gas)?,
            receipts_root: asc_new(heap, &self.receipts_root, gas)?,
            number: asc_new(heap, &BigInt::from(self.number), gas)?,
            gas_used: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_used), gas)?,
            gas_limit: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_limit), gas)?,
            timestamp: asc_new(heap, &BigInt::from_unsigned_u256(&self.timestamp), gas)?,
            difficulty: asc_new(heap, &BigInt::from_unsigned_u256(&self.difficulty), gas)?,
            total_difficulty: asc_new(
                heap,
                &BigInt::from_unsigned_u256(&self.total_difficulty),
                gas,
            )?,
            size: self
                .size
                .map(|size| asc_new(heap, &BigInt::from_unsigned_u256(&size), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
        })
    }
}

impl ToAscObj<AscEthereumBlock_0_0_6> for EthereumBlockData {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumBlock_0_0_6, HostExportError> {
        Ok(AscEthereumBlock_0_0_6 {
            hash: asc_new(heap, &self.hash, gas)?,
            parent_hash: asc_new(heap, &self.parent_hash, gas)?,
            uncles_hash: asc_new(heap, &self.uncles_hash, gas)?,
            author: asc_new(heap, &self.author, gas)?,
            state_root: asc_new(heap, &self.state_root, gas)?,
            transactions_root: asc_new(heap, &self.transactions_root, gas)?,
            receipts_root: asc_new(heap, &self.receipts_root, gas)?,
            number: asc_new(heap, &BigInt::from(self.number), gas)?,
            gas_used: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_used), gas)?,
            gas_limit: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_limit), gas)?,
            timestamp: asc_new(heap, &BigInt::from_unsigned_u256(&self.timestamp), gas)?,
            difficulty: asc_new(heap, &BigInt::from_unsigned_u256(&self.difficulty), gas)?,
            total_difficulty: asc_new(
                heap,
                &BigInt::from_unsigned_u256(&self.total_difficulty),
                gas,
            )?,
            size: self
                .size
                .map(|size| asc_new(heap, &BigInt::from_unsigned_u256(&size), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            base_fee_per_block: self
                .base_fee_per_gas
                .map(|base_fee| asc_new(heap, &BigInt::from_unsigned_u256(&base_fee), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
        })
    }
}

impl ToAscObj<AscEthereumTransaction_0_0_1> for EthereumTransactionData {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumTransaction_0_0_1, HostExportError> {
        Ok(AscEthereumTransaction_0_0_1 {
            hash: asc_new(heap, &self.hash, gas)?,
            index: asc_new(heap, &BigInt::from_unsigned_u128(self.index), gas)?,
            from: asc_new(heap, &self.from, gas)?,
            to: self
                .to
                .map(|to| asc_new(heap, &to, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            value: asc_new(heap, &BigInt::from_unsigned_u256(&self.value), gas)?,
            gas_limit: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_limit), gas)?,
            gas_price: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_price), gas)?,
        })
    }
}

impl ToAscObj<AscEthereumTransaction_0_0_2> for EthereumTransactionData {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumTransaction_0_0_2, HostExportError> {
        Ok(AscEthereumTransaction_0_0_2 {
            hash: asc_new(heap, &self.hash, gas)?,
            index: asc_new(heap, &BigInt::from_unsigned_u128(self.index), gas)?,
            from: asc_new(heap, &self.from, gas)?,
            to: self
                .to
                .map(|to| asc_new(heap, &to, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            value: asc_new(heap, &BigInt::from_unsigned_u256(&self.value), gas)?,
            gas_limit: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_limit), gas)?,
            gas_price: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_price), gas)?,
            input: asc_new(heap, &*self.input, gas)?,
        })
    }
}

impl ToAscObj<AscEthereumTransaction_0_0_6> for EthereumTransactionData {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumTransaction_0_0_6, HostExportError> {
        Ok(AscEthereumTransaction_0_0_6 {
            hash: asc_new(heap, &self.hash, gas)?,
            index: asc_new(heap, &BigInt::from_unsigned_u128(self.index), gas)?,
            from: asc_new(heap, &self.from, gas)?,
            to: self
                .to
                .map(|to| asc_new(heap, &to, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            value: asc_new(heap, &BigInt::from_unsigned_u256(&self.value), gas)?,
            gas_limit: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_limit), gas)?,
            gas_price: asc_new(heap, &BigInt::from_unsigned_u256(&self.gas_price), gas)?,
            input: asc_new(heap, &*self.input, gas)?,
            nonce: asc_new(heap, &BigInt::from_unsigned_u256(&self.nonce), gas)?,
        })
    }
}

impl<T, B> ToAscObj<AscEthereumEvent<T, B>> for EthereumEventData
where
    T: AscType + AscIndexId,
    B: AscType + AscIndexId,
    EthereumTransactionData: ToAscObj<T>,
    EthereumBlockData: ToAscObj<B>,
{
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumEvent<T, B>, HostExportError> {
        Ok(AscEthereumEvent {
            address: asc_new(heap, &self.address, gas)?,
            log_index: asc_new(heap, &BigInt::from_unsigned_u256(&self.log_index), gas)?,
            transaction_log_index: asc_new(
                heap,
                &BigInt::from_unsigned_u256(&self.transaction_log_index),
                gas,
            )?,
            log_type: self
                .log_type
                .clone()
                .map(|log_type| asc_new(heap, &log_type, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            block: asc_new::<B, EthereumBlockData, _>(heap, &self.block, gas)?,
            transaction: asc_new::<T, EthereumTransactionData, _>(heap, &self.transaction, gas)?,
            params: asc_new(heap, &self.params, gas)?,
        })
    }
}

impl<T, B> ToAscObj<AscEthereumEvent_0_0_7<T, B>>
    for (EthereumEventData, Option<&TransactionReceipt>)
where
    T: AscType + AscIndexId,
    B: AscType + AscIndexId,
    EthereumTransactionData: ToAscObj<T>,
    EthereumBlockData: ToAscObj<B>,
{
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumEvent_0_0_7<T, B>, HostExportError> {
        let (event_data, optional_receipt) = self;
        let AscEthereumEvent {
            address,
            log_index,
            transaction_log_index,
            log_type,
            block,
            transaction,
            params,
        } = event_data.to_asc_obj(heap, gas)?;
        let receipt = if let Some(receipt_data) = optional_receipt {
            asc_new(heap, receipt_data, gas)?
        } else {
            AscPtr::null()
        };
        Ok(AscEthereumEvent_0_0_7 {
            address,
            log_index,
            transaction_log_index,
            log_type,
            block,
            transaction,
            params,
            receipt,
        })
    }
}

impl ToAscObj<AscEthereumLog> for Log {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumLog, HostExportError> {
        Ok(AscEthereumLog {
            address: asc_new(heap, &self.address, gas)?,
            topics: asc_new(heap, &self.topics, gas)?,
            data: asc_new(heap, self.data.0.as_slice(), gas)?,
            block_hash: self
                .block_hash
                .map(|block_hash| asc_new(heap, &block_hash, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            block_number: self
                .block_number
                .map(|block_number| asc_new(heap, &BigInt::from(block_number), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            transaction_hash: self
                .transaction_hash
                .map(|txn_hash| asc_new(heap, &txn_hash, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            transaction_index: self
                .transaction_index
                .map(|txn_index| asc_new(heap, &BigInt::from(txn_index), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            log_index: self
                .log_index
                .map(|log_index| asc_new(heap, &BigInt::from_unsigned_u256(&log_index), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            transaction_log_index: self
                .transaction_log_index
                .map(|index| asc_new(heap, &BigInt::from_unsigned_u256(&index), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            log_type: self
                .log_type
                .as_ref()
                .map(|log_type| asc_new(heap, &log_type, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            removed: self
                .removed
                .map(|removed| asc_new(heap, &AscWrapped { inner: removed }, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
        })
    }
}

impl ToAscObj<AscEthereumTransactionReceipt> for &TransactionReceipt {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumTransactionReceipt, HostExportError> {
        Ok(AscEthereumTransactionReceipt {
            transaction_hash: asc_new(heap, &self.transaction_hash, gas)?,
            transaction_index: asc_new(heap, &BigInt::from(self.transaction_index), gas)?,
            block_hash: self
                .block_hash
                .map(|block_hash| asc_new(heap, &block_hash, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            block_number: self
                .block_number
                .map(|block_number| asc_new(heap, &BigInt::from(block_number), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            cumulative_gas_used: asc_new(
                heap,
                &BigInt::from_unsigned_u256(&self.cumulative_gas_used),
                gas,
            )?,
            gas_used: self
                .gas_used
                .map(|gas_used| asc_new(heap, &BigInt::from_unsigned_u256(&gas_used), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            contract_address: self
                .contract_address
                .map(|contract_address| asc_new(heap, &contract_address, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            logs: asc_new(heap, &self.logs, gas)?,
            status: self
                .status
                .map(|status| asc_new(heap, &BigInt::from(status), gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            root: self
                .root
                .map(|root| asc_new(heap, &root, gas))
                .unwrap_or(Ok(AscPtr::null()))?,
            logs_bloom: asc_new(heap, self.logs_bloom.as_bytes(), gas)?,
        })
    }
}

impl ToAscObj<AscEthereumCall> for EthereumCallData {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscEthereumCall, HostExportError> {
        Ok(AscEthereumCall {
            address: asc_new(heap, &self.to, gas)?,
            block: asc_new(heap, &self.block, gas)?,
            transaction: asc_new(heap, &self.transaction, gas)?,
            inputs: asc_new(heap, &self.inputs, gas)?,
            outputs: asc_new(heap, &self.outputs, gas)?,
        })
    }
}

impl ToAscObj<AscEthereumCall_0_0_3<AscEthereumTransaction_0_0_2, AscEthereumBlock>>
    for EthereumCallData
{
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<
        AscEthereumCall_0_0_3<AscEthereumTransaction_0_0_2, AscEthereumBlock>,
        HostExportError,
    > {
        Ok(AscEthereumCall_0_0_3 {
            to: asc_new(heap, &self.to, gas)?,
            from: asc_new(heap, &self.from, gas)?,
            block: asc_new(heap, &self.block, gas)?,
            transaction: asc_new(heap, &self.transaction, gas)?,
            inputs: asc_new(heap, &self.inputs, gas)?,
            outputs: asc_new(heap, &self.outputs, gas)?,
        })
    }
}

impl ToAscObj<AscEthereumCall_0_0_3<AscEthereumTransaction_0_0_6, AscEthereumBlock_0_0_6>>
    for EthereumCallData
{
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<
        AscEthereumCall_0_0_3<AscEthereumTransaction_0_0_6, AscEthereumBlock_0_0_6>,
        HostExportError,
    > {
        Ok(AscEthereumCall_0_0_3 {
            to: asc_new(heap, &self.to, gas)?,
            from: asc_new(heap, &self.from, gas)?,
            block: asc_new(heap, &self.block, gas)?,
            transaction: asc_new(heap, &self.transaction, gas)?,
            inputs: asc_new(heap, &self.inputs, gas)?,
            outputs: asc_new(heap, &self.outputs, gas)?,
        })
    }
}

impl ToAscObj<AscLogParam> for ethabi::LogParam {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscLogParam, HostExportError> {
        Ok(AscLogParam {
            name: asc_new(heap, self.name.as_str(), gas)?,
            value: asc_new(heap, &self.value, gas)?,
        })
    }
}
