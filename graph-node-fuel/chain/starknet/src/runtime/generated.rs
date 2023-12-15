use graph::runtime::{
    AscIndexId, AscPtr, AscType, AscValue, DeterministicHostError, IndexForAscTypeId,
};
use graph::semver::Version;
use graph_runtime_derive::AscType;
use graph_runtime_wasm::asc_abi::class::{Array, AscBigInt, AscEnum, Uint8Array};

pub struct AscBytesArray(pub(crate) Array<AscPtr<Uint8Array>>);

impl AscType for AscBytesArray {
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

impl AscIndexId for AscBytesArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::StarknetArrayBytes;
}

pub struct AscTransactionTypeEnum(pub(crate) AscEnum<AscTransactionType>);

impl AscType for AscTransactionTypeEnum {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        Ok(Self(AscEnum::from_asc_bytes(asc_obj, api_version)?))
    }
}

impl AscIndexId for AscTransactionTypeEnum {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::StarknetTransactionTypeEnum;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscBlock {
    pub number: AscPtr<AscBigInt>,
    pub hash: AscPtr<Uint8Array>,
    pub prev_hash: AscPtr<Uint8Array>,
    pub timestamp: AscPtr<AscBigInt>,
}

impl AscIndexId for AscBlock {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::StarknetBlock;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscTransaction {
    pub r#type: AscPtr<AscTransactionTypeEnum>,
    pub hash: AscPtr<Uint8Array>,
}

impl AscIndexId for AscTransaction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::StarknetTransaction;
}

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub(crate) enum AscTransactionType {
    Deploy,
    InvokeFunction,
    Declare,
    L1Handler,
    DeployAccount,
}

impl AscValue for AscTransactionType {}

impl Default for AscTransactionType {
    fn default() -> Self {
        Self::Deploy
    }
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEvent {
    pub from_addr: AscPtr<Uint8Array>,
    pub keys: AscPtr<AscBytesArray>,
    pub data: AscPtr<AscBytesArray>,
    pub block: AscPtr<AscBlock>,
    pub transaction: AscPtr<AscTransaction>,
}

impl AscIndexId for AscEvent {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::StarknetEvent;
}
