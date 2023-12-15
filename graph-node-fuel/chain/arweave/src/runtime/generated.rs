use graph::runtime::{AscIndexId, AscPtr, AscType, DeterministicHostError, IndexForAscTypeId};
use graph::semver::Version;
use graph_runtime_derive::AscType;
use graph_runtime_wasm::asc_abi::class::{Array, AscString, Uint8Array};

#[repr(C)]
#[derive(AscType, Default)]
pub struct AscBlock {
    pub timestamp: u64,
    pub last_retarget: u64,
    pub height: u64,
    pub indep_hash: AscPtr<Uint8Array>,
    pub nonce: AscPtr<Uint8Array>,
    pub previous_block: AscPtr<Uint8Array>,
    pub diff: AscPtr<Uint8Array>,
    pub hash: AscPtr<Uint8Array>,
    pub tx_root: AscPtr<Uint8Array>,
    pub txs: AscPtr<AscTransactionArray>,
    pub wallet_list: AscPtr<Uint8Array>,
    pub reward_addr: AscPtr<Uint8Array>,
    pub tags: AscPtr<AscTagArray>,
    pub reward_pool: AscPtr<Uint8Array>,
    pub weave_size: AscPtr<Uint8Array>,
    pub block_size: AscPtr<Uint8Array>,
    pub cumulative_diff: AscPtr<Uint8Array>,
    pub hash_list_merkle: AscPtr<Uint8Array>,
    pub poa: AscPtr<AscProofOfAccess>,
}

impl AscIndexId for AscBlock {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArweaveBlock;
}

#[repr(C)]
#[derive(AscType)]
pub struct AscProofOfAccess {
    pub option: AscPtr<AscString>,
    pub tx_path: AscPtr<Uint8Array>,
    pub data_path: AscPtr<Uint8Array>,
    pub chunk: AscPtr<Uint8Array>,
}

impl AscIndexId for AscProofOfAccess {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArweaveProofOfAccess;
}

#[repr(C)]
#[derive(AscType)]
pub struct AscTransaction {
    pub format: u32,
    pub id: AscPtr<Uint8Array>,
    pub last_tx: AscPtr<Uint8Array>,
    pub owner: AscPtr<Uint8Array>,
    pub tags: AscPtr<AscTagArray>,
    pub target: AscPtr<Uint8Array>,
    pub quantity: AscPtr<Uint8Array>,
    pub data: AscPtr<Uint8Array>,
    pub data_size: AscPtr<Uint8Array>,
    pub data_root: AscPtr<Uint8Array>,
    pub signature: AscPtr<Uint8Array>,
    pub reward: AscPtr<Uint8Array>,
}

impl AscIndexId for AscTransaction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArweaveTransaction;
}

#[repr(C)]
#[derive(AscType)]
pub struct AscTag {
    pub name: AscPtr<Uint8Array>,
    pub value: AscPtr<Uint8Array>,
}

impl AscIndexId for AscTag {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArweaveTag;
}

#[repr(C)]
pub struct AscTransactionArray(pub(crate) Array<AscPtr<Uint8Array>>);

impl AscType for AscTransactionArray {
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

impl AscIndexId for AscTransactionArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArweaveTransactionArray;
}

#[repr(C)]
pub struct AscTagArray(pub(crate) Array<AscPtr<AscTag>>);

impl AscType for AscTagArray {
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

impl AscIndexId for AscTagArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArweaveTagArray;
}

#[repr(C)]
#[derive(AscType)]
pub struct AscTransactionWithBlockPtr {
    pub tx: AscPtr<AscTransaction>,
    pub block: AscPtr<AscBlock>,
}

impl AscIndexId for AscTransactionWithBlockPtr {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArweaveTransactionWithBlockPtr;
}
