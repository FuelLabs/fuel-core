use graph::runtime::{
    AscIndexId, AscPtr, AscType, AscValue, DeterministicHostError, IndexForAscTypeId,
};
use graph::semver::Version;
use graph_runtime_derive::AscType;
use graph_runtime_wasm::asc_abi::class::{Array, AscBigInt, AscEnum, AscString, Uint8Array};

pub(crate) type AscCryptoHash = Uint8Array;
pub(crate) type AscAccountId = AscString;
pub(crate) type AscBlockHeight = u64;
pub(crate) type AscBalance = AscBigInt;
pub(crate) type AscGas = u64;
pub(crate) type AscShardId = u64;
pub(crate) type AscNumBlocks = u64;
pub(crate) type AscProtocolVersion = u32;

pub struct AscDataReceiverArray(pub(crate) Array<AscPtr<AscDataReceiver>>);

impl AscType for AscDataReceiverArray {
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

impl AscIndexId for AscDataReceiverArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearArrayDataReceiver;
}

pub struct AscCryptoHashArray(pub(crate) Array<AscPtr<AscCryptoHash>>);

impl AscType for AscCryptoHashArray {
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

impl AscIndexId for AscCryptoHashArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearArrayCryptoHash;
}

pub struct AscActionEnumArray(pub(crate) Array<AscPtr<AscActionEnum>>);

impl AscType for AscActionEnumArray {
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

impl AscIndexId for AscActionEnumArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearArrayActionEnum;
}

pub struct AscMerklePathItemArray(pub(crate) Array<AscPtr<AscMerklePathItem>>);

impl AscType for AscMerklePathItemArray {
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

impl AscIndexId for AscMerklePathItemArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearArrayMerklePathItem;
}

pub struct AscValidatorStakeArray(pub(crate) Array<AscPtr<AscValidatorStake>>);

impl AscType for AscValidatorStakeArray {
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

impl AscIndexId for AscValidatorStakeArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearArrayValidatorStake;
}

pub struct AscSlashedValidatorArray(pub(crate) Array<AscPtr<AscSlashedValidator>>);

impl AscType for AscSlashedValidatorArray {
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

impl AscIndexId for AscSlashedValidatorArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearArraySlashedValidator;
}

pub struct AscSignatureArray(pub(crate) Array<AscPtr<AscSignature>>);

impl AscType for AscSignatureArray {
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

impl AscIndexId for AscSignatureArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearArraySignature;
}

pub struct AscChunkHeaderArray(pub(crate) Array<AscPtr<AscChunkHeader>>);

impl AscType for AscChunkHeaderArray {
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

impl AscIndexId for AscChunkHeaderArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearArrayChunkHeader;
}

pub struct AscAccessKeyPermissionEnum(pub(crate) AscEnum<AscAccessKeyPermissionKind>);

impl AscType for AscAccessKeyPermissionEnum {
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

impl AscIndexId for AscAccessKeyPermissionEnum {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearAccessKeyPermissionEnum;
}

pub struct AscActionEnum(pub(crate) AscEnum<AscActionKind>);

impl AscType for AscActionEnum {
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

impl AscIndexId for AscActionEnum {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearActionEnum;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscPublicKey {
    pub kind: i32,
    pub bytes: AscPtr<Uint8Array>,
}

impl AscIndexId for AscPublicKey {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearPublicKey;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscSignature {
    pub kind: i32,
    pub bytes: AscPtr<Uint8Array>,
}

impl AscIndexId for AscSignature {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearSignature;
}

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub(crate) enum AscAccessKeyPermissionKind {
    FunctionCall,
    FullAccess,
}

impl AscValue for AscAccessKeyPermissionKind {}

impl Default for AscAccessKeyPermissionKind {
    fn default() -> Self {
        Self::FunctionCall
    }
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscFunctionCallPermission {
    pub allowance: AscPtr<AscBigInt>,
    pub receiver_id: AscPtr<AscString>,
    pub method_names: AscPtr<Array<AscPtr<AscString>>>,
}

impl AscIndexId for AscFunctionCallPermission {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearFunctionCallPermission;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscFullAccessPermission {}

impl AscIndexId for AscFullAccessPermission {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearFullAccessPermission;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscAccessKey {
    pub nonce: u64,
    pub permission: AscPtr<AscAccessKeyPermissionEnum>,

    // It seems that is impossible to correctly order fields in this struct
    // so that Rust packs it tighly without padding. So we add 4 bytes of padding
    // ourself.
    //
    // This is a bit problematic because AssemblyScript actually is ok with 12 bytes
    // and is fully packed. Seems like a differences between alignment for `repr(C)` and
    // AssemblyScript.
    pub(crate) _padding: u32,
}

impl AscIndexId for AscAccessKey {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearAccessKey;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscDataReceiver {
    pub data_id: AscPtr<AscCryptoHash>,
    pub receiver_id: AscPtr<AscString>,
}

impl AscIndexId for AscDataReceiver {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearDataReceiver;
}

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub(crate) enum AscActionKind {
    CreateAccount,
    DeployContract,
    FunctionCall,
    Transfer,
    Stake,
    AddKey,
    DeleteKey,
    DeleteAccount,
}

impl AscValue for AscActionKind {}

impl Default for AscActionKind {
    fn default() -> Self {
        Self::CreateAccount
    }
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscCreateAccountAction {}

impl AscIndexId for AscCreateAccountAction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearCreateAccountAction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscDeployContractAction {
    pub code: AscPtr<Uint8Array>,
}

impl AscIndexId for AscDeployContractAction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearDeployContractAction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscFunctionCallAction {
    pub method_name: AscPtr<AscString>,
    pub args: AscPtr<Uint8Array>,
    pub gas: u64,
    pub deposit: AscPtr<AscBigInt>,

    // It seems that is impossible to correctly order fields in this struct
    // so that Rust packs it tighly without padding. So we add 4 bytes of padding
    // ourself.
    //
    // This is a bit problematic because AssemblyScript actually is ok with 20 bytes
    // and is fully packed. Seems like a differences between alignment for `repr(C)` and
    // AssemblyScript.
    pub(crate) _padding: u32,
}

impl AscIndexId for AscFunctionCallAction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearFunctionCallAction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscTransferAction {
    pub deposit: AscPtr<AscBigInt>,
}

impl AscIndexId for AscTransferAction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearTransferAction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscStakeAction {
    pub stake: AscPtr<AscBalance>,
    pub public_key: AscPtr<AscPublicKey>,
}

impl AscIndexId for AscStakeAction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearStakeAction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscAddKeyAction {
    pub public_key: AscPtr<AscPublicKey>,
    pub access_key: AscPtr<AscAccessKey>,
}

impl AscIndexId for AscAddKeyAction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearAddKeyAction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscDeleteKeyAction {
    pub public_key: AscPtr<AscPublicKey>,
}

impl AscIndexId for AscDeleteKeyAction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearDeleteKeyAction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscDeleteAccountAction {
    pub beneficiary_id: AscPtr<AscAccountId>,
}

impl AscIndexId for AscDeleteAccountAction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearDeleteAccountAction;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscActionReceipt {
    pub predecessor_id: AscPtr<AscString>,
    pub receiver_id: AscPtr<AscString>,
    pub id: AscPtr<AscCryptoHash>,
    pub signer_id: AscPtr<AscString>,
    pub signer_public_key: AscPtr<AscPublicKey>,
    pub gas_price: AscPtr<AscBigInt>,
    pub output_data_receivers: AscPtr<AscDataReceiverArray>,
    pub input_data_ids: AscPtr<AscCryptoHashArray>,
    pub actions: AscPtr<AscActionEnumArray>,
}

impl AscIndexId for AscActionReceipt {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearActionReceipt;
}

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub(crate) enum AscSuccessStatusKind {
    Value,
    ReceiptId,
}

impl AscValue for AscSuccessStatusKind {}

impl Default for AscSuccessStatusKind {
    fn default() -> Self {
        Self::Value
    }
}

pub struct AscSuccessStatusEnum(pub(crate) AscEnum<AscSuccessStatusKind>);

impl AscType for AscSuccessStatusEnum {
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

impl AscIndexId for AscSuccessStatusEnum {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearSuccessStatusEnum;
}

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub(crate) enum AscDirection {
    Left,
    Right,
}

impl AscValue for AscDirection {}

impl Default for AscDirection {
    fn default() -> Self {
        Self::Left
    }
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscMerklePathItem {
    pub hash: AscPtr<AscCryptoHash>,
    pub direction: AscDirection,
}

impl AscIndexId for AscMerklePathItem {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearMerklePathItem;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscExecutionOutcome {
    pub gas_burnt: u64,
    pub proof: AscPtr<AscMerklePathItemArray>,
    pub block_hash: AscPtr<AscCryptoHash>,
    pub id: AscPtr<AscCryptoHash>,
    pub logs: AscPtr<Array<AscPtr<AscString>>>,
    pub receipt_ids: AscPtr<AscCryptoHashArray>,
    pub tokens_burnt: AscPtr<AscBigInt>,
    pub executor_id: AscPtr<AscString>,
    pub status: AscPtr<AscSuccessStatusEnum>,
}

impl AscIndexId for AscExecutionOutcome {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearExecutionOutcome;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscSlashedValidator {
    pub account_id: AscPtr<AscAccountId>,
    pub is_double_sign: bool,
}

impl AscIndexId for AscSlashedValidator {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearSlashedValidator;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscBlockHeader {
    pub height: AscBlockHeight,
    pub prev_height: AscBlockHeight,
    pub block_ordinal: AscNumBlocks,
    pub epoch_id: AscPtr<AscCryptoHash>,
    pub next_epoch_id: AscPtr<AscCryptoHash>,
    pub chunks_included: u64,
    pub hash: AscPtr<AscCryptoHash>,
    pub prev_hash: AscPtr<AscCryptoHash>,
    pub timestamp_nanosec: u64,
    pub prev_state_root: AscPtr<AscCryptoHash>,
    pub chunk_receipts_root: AscPtr<AscCryptoHash>,
    pub chunk_headers_root: AscPtr<AscCryptoHash>,
    pub chunk_tx_root: AscPtr<AscCryptoHash>,
    pub outcome_root: AscPtr<AscCryptoHash>,
    pub challenges_root: AscPtr<AscCryptoHash>,
    pub random_value: AscPtr<AscCryptoHash>,
    pub validator_proposals: AscPtr<AscValidatorStakeArray>,
    pub chunk_mask: AscPtr<Array<bool>>,
    pub gas_price: AscPtr<AscBalance>,
    pub total_supply: AscPtr<AscBalance>,
    pub challenges_result: AscPtr<AscSlashedValidatorArray>,
    pub last_final_block: AscPtr<AscCryptoHash>,
    pub last_ds_final_block: AscPtr<AscCryptoHash>,
    pub next_bp_hash: AscPtr<AscCryptoHash>,
    pub block_merkle_root: AscPtr<AscCryptoHash>,
    pub epoch_sync_data_hash: AscPtr<AscCryptoHash>,
    pub approvals: AscPtr<AscSignatureArray>,
    pub signature: AscPtr<AscSignature>,
    pub latest_protocol_version: AscProtocolVersion,
}

impl AscIndexId for AscBlockHeader {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearBlockHeader;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscValidatorStake {
    pub account_id: AscPtr<AscAccountId>,
    pub public_key: AscPtr<AscPublicKey>,
    pub stake: AscPtr<AscBalance>,
}

impl AscIndexId for AscValidatorStake {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearValidatorStake;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscChunkHeader {
    pub encoded_length: u64,
    pub gas_used: AscGas,
    pub gas_limit: AscGas,
    pub shard_id: AscShardId,
    pub height_created: AscBlockHeight,
    pub height_included: AscBlockHeight,
    pub chunk_hash: AscPtr<AscCryptoHash>,
    pub signature: AscPtr<AscSignature>,
    pub prev_block_hash: AscPtr<AscCryptoHash>,
    pub prev_state_root: AscPtr<AscCryptoHash>,
    pub encoded_merkle_root: AscPtr<AscCryptoHash>,
    pub balance_burnt: AscPtr<AscBalance>,
    pub outgoing_receipts_root: AscPtr<AscCryptoHash>,
    pub tx_root: AscPtr<AscCryptoHash>,
    pub validator_proposals: AscPtr<AscValidatorStakeArray>,

    // It seems that is impossible to correctly order fields in this struct
    // so that Rust packs it tighly without padding. So we add 4 bytes of padding
    // ourself.
    //
    // This is a bit problematic because AssemblyScript actually is ok with 84 bytes
    // and is fully packed. Seems like a differences between alignment for `repr(C)` and
    // AssemblyScript.
    pub(crate) _padding: u32,
}

impl AscIndexId for AscChunkHeader {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearChunkHeader;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscBlock {
    pub author: AscPtr<AscAccountId>,
    pub header: AscPtr<AscBlockHeader>,
    pub chunks: AscPtr<AscChunkHeaderArray>,
}

impl AscIndexId for AscBlock {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearBlock;
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscReceiptWithOutcome {
    pub outcome: AscPtr<AscExecutionOutcome>,
    pub receipt: AscPtr<AscActionReceipt>,
    pub block: AscPtr<AscBlock>,
}

impl AscIndexId for AscReceiptWithOutcome {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::NearReceiptWithOutcome;
}
