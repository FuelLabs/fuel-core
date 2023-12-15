//! Facilities for creating and reading objects on the memory of an AssemblyScript (Asc) WASM
//! module. Objects are passed through the `asc_new` and `asc_get` methods of an `AscHeap`
//! implementation. These methods take types that implement `To`/`FromAscObj` and are therefore
//! convertible to/from an `AscType`.

pub mod gas;

mod asc_heap;
mod asc_ptr;

pub use asc_heap::{
    asc_get, asc_new, asc_new_or_missing, asc_new_or_null, AscHeap, FromAscObj, ToAscObj,
};
pub use asc_ptr::AscPtr;

use anyhow::Error;
use semver::Version;
use std::convert::TryInto;
use std::fmt;
use std::mem::size_of;

use self::gas::GasCounter;

/// Marker trait for AssemblyScript types that the id should
/// be in the header.
pub trait AscIndexId {
    /// Constant string with the name of the type in AssemblyScript.
    /// This is used to get the identifier for the type in memory layout.
    /// Info about memory layout:
    /// https://www.assemblyscript.org/memory.html#common-header-layout.
    /// Info about identifier (`idof<T>`):
    /// https://www.assemblyscript.org/garbage-collection.html#runtime-interface
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId;
}

/// A type that has a direct correspondence to an Asc type.
///
/// This can be derived for structs that are `#[repr(C)]`, contain no padding
/// and whose fields are all `AscValue`. Enums can derive if they are `#[repr(u32)]`.
///
/// Special classes like `ArrayBuffer` use custom impls.
///
/// See https://github.com/graphprotocol/graph-node/issues/607 for more considerations.
pub trait AscType: Sized {
    /// Transform the Rust representation of this instance into an sequence of
    /// bytes that is precisely the memory layout of a corresponding Asc instance.
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError>;

    /// The Rust representation of an Asc object as layed out in Asc memory.
    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError>;

    fn content_len(&self, asc_bytes: &[u8]) -> usize {
        asc_bytes.len()
    }

    /// Size of the corresponding Asc instance in bytes.
    /// Only used for version <= 0.0.3.
    fn asc_size<H: AscHeap + ?Sized>(
        _ptr: AscPtr<Self>,
        _heap: &H,
        _gas: &GasCounter,
    ) -> Result<u32, DeterministicHostError> {
        Ok(std::mem::size_of::<Self>() as u32)
    }
}

// Only implemented because of structs that derive AscType and
// contain fields that are PhantomData.
impl<T> AscType for std::marker::PhantomData<T> {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        Ok(vec![])
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        _api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        assert!(asc_obj.is_empty());

        Ok(Self)
    }
}

/// An Asc primitive or an `AscPtr` into the Asc heap. A type marked as
/// `AscValue` must have the same byte representation in Rust and Asc, including
/// same size, and size must be equal to alignment.
pub trait AscValue: AscType + Copy + Default {}

impl AscType for bool {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        Ok(vec![*self as u8])
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        _api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        if asc_obj.len() != 1 {
            Err(DeterministicHostError::from(anyhow::anyhow!(
                "Incorrect size for bool. Expected 1, got {},",
                asc_obj.len()
            )))
        } else {
            Ok(asc_obj[0] != 0)
        }
    }
}

impl AscValue for bool {}
impl<T> AscValue for AscPtr<T> {}

macro_rules! impl_asc_type {
    ($($T:ty),*) => {
        $(
            impl AscType for $T {
                fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
                    Ok(self.to_le_bytes().to_vec())
                }

                fn from_asc_bytes(asc_obj: &[u8], _api_version: &Version) -> Result<Self, DeterministicHostError> {
                    let bytes = asc_obj.try_into().map_err(|_| {
                        DeterministicHostError::from(anyhow::anyhow!(
                            "Incorrect size for {}. Expected {}, got {},",
                            stringify!($T),
                            size_of::<Self>(),
                            asc_obj.len()
                        ))
                    })?;

                    Ok(Self::from_le_bytes(bytes))
                }
            }

            impl AscValue for $T {}
        )*
    };
}

impl_asc_type!(u8, u16, u32, u64, i8, i32, i64, f32, f64);

/// Contains type IDs and their discriminants for every blockchain supported by Graph-Node.
///
/// Each variant corresponds to the unique ID of an AssemblyScript concrete class used in the
/// [`runtime`].
///
/// # Rules for updating this enum
///
/// 1 .The discriminants must have the same value as their counterparts in `TypeId` enum from
///    graph-ts' `global` module. If not, the runtime will fail to determine the correct class
///    during allocation.
/// 2. Each supported blockchain has a reserved space of 1,000 contiguous variants.
/// 3. Once defined, items and their discriminants cannot be changed, as this would break running
///    subgraphs compiled in previous versions of this representation.
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum IndexForAscTypeId {
    // Ethereum type IDs
    String = 0,
    ArrayBuffer = 1,
    Int8Array = 2,
    Int16Array = 3,
    Int32Array = 4,
    Int64Array = 5,
    Uint8Array = 6,
    Uint16Array = 7,
    Uint32Array = 8,
    Uint64Array = 9,
    Float32Array = 10,
    Float64Array = 11,
    BigDecimal = 12,
    ArrayBool = 13,
    ArrayUint8Array = 14,
    ArrayEthereumValue = 15,
    ArrayStoreValue = 16,
    ArrayJsonValue = 17,
    ArrayString = 18,
    ArrayEventParam = 19,
    ArrayTypedMapEntryStringJsonValue = 20,
    ArrayTypedMapEntryStringStoreValue = 21,
    SmartContractCall = 22,
    EventParam = 23,
    EthereumTransaction = 24,
    EthereumBlock = 25,
    EthereumCall = 26,
    WrappedTypedMapStringJsonValue = 27,
    WrappedBool = 28,
    WrappedJsonValue = 29,
    EthereumValue = 30,
    StoreValue = 31,
    JsonValue = 32,
    EthereumEvent = 33,
    TypedMapEntryStringStoreValue = 34,
    TypedMapEntryStringJsonValue = 35,
    TypedMapStringStoreValue = 36,
    TypedMapStringJsonValue = 37,
    TypedMapStringTypedMapStringJsonValue = 38,
    ResultTypedMapStringJsonValueBool = 39,
    ResultJsonValueBool = 40,
    ArrayU8 = 41,
    ArrayU16 = 42,
    ArrayU32 = 43,
    ArrayU64 = 44,
    ArrayI8 = 45,
    ArrayI16 = 46,
    ArrayI32 = 47,
    ArrayI64 = 48,
    ArrayF32 = 49,
    ArrayF64 = 50,
    ArrayBigDecimal = 51,

    // Near Type IDs
    NearArrayDataReceiver = 52,
    NearArrayCryptoHash = 53,
    NearArrayActionEnum = 54,
    NearArrayMerklePathItem = 55,
    NearArrayValidatorStake = 56,
    NearArraySlashedValidator = 57,
    NearArraySignature = 58,
    NearArrayChunkHeader = 59,
    NearAccessKeyPermissionEnum = 60,
    NearActionEnum = 61,
    NearDirectionEnum = 62,
    NearPublicKey = 63,
    NearSignature = 64,
    NearFunctionCallPermission = 65,
    NearFullAccessPermission = 66,
    NearAccessKey = 67,
    NearDataReceiver = 68,
    NearCreateAccountAction = 69,
    NearDeployContractAction = 70,
    NearFunctionCallAction = 71,
    NearTransferAction = 72,
    NearStakeAction = 73,
    NearAddKeyAction = 74,
    NearDeleteKeyAction = 75,
    NearDeleteAccountAction = 76,
    NearActionReceipt = 77,
    NearSuccessStatusEnum = 78,
    NearMerklePathItem = 79,
    NearExecutionOutcome = 80,
    NearSlashedValidator = 81,
    NearBlockHeader = 82,
    NearValidatorStake = 83,
    NearChunkHeader = 84,
    NearBlock = 85,
    NearReceiptWithOutcome = 86,
    // Reserved discriminant space for more Near type IDs: [87, 999]:
    // Continue to add more Near type IDs here.
    // e.g.:
    // NextNearType = 87,
    // AnotherNearType = 88,
    // ...
    // LastNearType = 999,

    // Reserved discriminant space for more Ethereum type IDs: [1000, 1499]
    TransactionReceipt = 1000,
    Log = 1001,
    ArrayH256 = 1002,
    ArrayLog = 1003,
    ArrayTypedMapStringStoreValue = 1004,
    // Continue to add more Ethereum type IDs here.
    // e.g.:
    // NextEthereumType = 1004,
    // AnotherEthereumType = 1005,
    // ...
    // LastEthereumType = 1499,

    // Reserved discriminant space for Cosmos type IDs: [1,500, 2,499]
    CosmosAny = 1500,
    CosmosAnyArray = 1501,
    CosmosBytesArray = 1502,
    CosmosCoinArray = 1503,
    CosmosCommitSigArray = 1504,
    CosmosEventArray = 1505,
    CosmosEventAttributeArray = 1506,
    CosmosEvidenceArray = 1507,
    CosmosModeInfoArray = 1508,
    CosmosSignerInfoArray = 1509,
    CosmosTxResultArray = 1510,
    CosmosValidatorArray = 1511,
    CosmosValidatorUpdateArray = 1512,
    CosmosAuthInfo = 1513,
    CosmosBlock = 1514,
    CosmosBlockId = 1515,
    CosmosBlockIdFlagEnum = 1516,
    CosmosBlockParams = 1517,
    CosmosCoin = 1518,
    CosmosCommit = 1519,
    CosmosCommitSig = 1520,
    CosmosCompactBitArray = 1521,
    CosmosConsensus = 1522,
    CosmosConsensusParams = 1523,
    CosmosDuplicateVoteEvidence = 1524,
    CosmosDuration = 1525,
    CosmosEvent = 1526,
    CosmosEventAttribute = 1527,
    CosmosEventData = 1528,
    CosmosEventVote = 1529,
    CosmosEvidence = 1530,
    CosmosEvidenceList = 1531,
    CosmosEvidenceParams = 1532,
    CosmosFee = 1533,
    CosmosHeader = 1534,
    CosmosHeaderOnlyBlock = 1535,
    CosmosLightBlock = 1536,
    CosmosLightClientAttackEvidence = 1537,
    CosmosModeInfo = 1538,
    CosmosModeInfoMulti = 1539,
    CosmosModeInfoSingle = 1540,
    CosmosPartSetHeader = 1541,
    CosmosPublicKey = 1542,
    CosmosResponseBeginBlock = 1543,
    CosmosResponseDeliverTx = 1544,
    CosmosResponseEndBlock = 1545,
    CosmosSignModeEnum = 1546,
    CosmosSignedHeader = 1547,
    CosmosSignedMsgTypeEnum = 1548,
    CosmosSignerInfo = 1549,
    CosmosTimestamp = 1550,
    CosmosTip = 1551,
    CosmosTransactionData = 1552,
    CosmosTx = 1553,
    CosmosTxBody = 1554,
    CosmosTxResult = 1555,
    CosmosValidator = 1556,
    CosmosValidatorParams = 1557,
    CosmosValidatorSet = 1558,
    CosmosValidatorSetUpdates = 1559,
    CosmosValidatorUpdate = 1560,
    CosmosVersionParams = 1561,
    CosmosMessageData = 1562,
    CosmosTransactionContext = 1563,
    // Continue to add more Cosmos type IDs here.
    // e.g.:
    // NextCosmosType = 1564,
    // AnotherCosmosType = 1565,
    // ...
    // LastCosmosType = 2499,

    // Arweave types
    ArweaveBlock = 2500,
    ArweaveProofOfAccess = 2501,
    ArweaveTag = 2502,
    ArweaveTagArray = 2503,
    ArweaveTransaction = 2504,
    ArweaveTransactionArray = 2505,
    ArweaveTransactionWithBlockPtr = 2506,
    // Continue to add more Arweave type IDs here.
    // e.g.:
    // NextArweaveType = 2507,
    // AnotherArweaveType = 2508,
    // ...
    // LastArweaveType = 3499,

    // StarkNet types
    StarknetBlock = 3500,
    StarknetTransaction = 3501,
    StarknetTransactionTypeEnum = 3502,
    StarknetEvent = 3503,
    StarknetArrayBytes = 3504,
    // Continue to add more StarkNet type IDs here.
    // e.g.:
    // NextStarknetType = 3505,
    // AnotherStarknetType = 3506,
    // ...
    // LastStarknetType = 4499,

    // Reserved discriminant space for a future blockchain type IDs: [4,500, 5,499]
    //
    // Generated with the following shell script:
    //
    // ```
    // grep -Po "(?<=IndexForAscTypeId::)IDENDIFIER_PREFIX.*\b" SRC_FILE | sort |uniq | awk 'BEGIN{count=2500} {sub("$", " = "count",", $1); count++} 1'
    // ```
    //
    // INSTRUCTIONS:
    // 1. Replace the IDENTIFIER_PREFIX and the SRC_FILE placeholders according to the blockchain
    //    name and implementation before running this script.
    // 2. Replace `3500` part with the first number of that blockchain's reserved discriminant space.
    // 3. Insert the output right before the end of this block.
    UnitTestNetworkUnitTestTypeU32 = u32::MAX - 7,
    UnitTestNetworkUnitTestTypeU32Array = u32::MAX - 6,

    UnitTestNetworkUnitTestTypeU16 = u32::MAX - 5,
    UnitTestNetworkUnitTestTypeU16Array = u32::MAX - 4,

    UnitTestNetworkUnitTestTypeI8 = u32::MAX - 3,
    UnitTestNetworkUnitTestTypeI8Array = u32::MAX - 2,

    UnitTestNetworkUnitTestTypeBool = u32::MAX - 1,
    UnitTestNetworkUnitTestTypeBoolArray = u32::MAX,
}

impl ToAscObj<u32> for IndexForAscTypeId {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        _heap: &mut H,
        _gas: &GasCounter,
    ) -> Result<u32, HostExportError> {
        Ok(*self as u32)
    }
}

#[derive(Debug)]
pub enum DeterministicHostError {
    Gas(Error),
    Other(Error),
}

impl DeterministicHostError {
    pub fn gas(e: Error) -> Self {
        DeterministicHostError::Gas(e)
    }

    pub fn inner(self) -> Error {
        match self {
            DeterministicHostError::Gas(e) | DeterministicHostError::Other(e) => e,
        }
    }
}

impl fmt::Display for DeterministicHostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeterministicHostError::Gas(e) | DeterministicHostError::Other(e) => e.fmt(f),
        }
    }
}

impl From<Error> for DeterministicHostError {
    fn from(e: Error) -> DeterministicHostError {
        DeterministicHostError::Other(e)
    }
}

impl std::error::Error for DeterministicHostError {}

#[derive(thiserror::Error, Debug)]
pub enum HostExportError {
    #[error("{0:#}")]
    Unknown(#[from] anyhow::Error),

    #[error("{0:#}")]
    PossibleReorg(anyhow::Error),

    #[error("{0:#}")]
    Deterministic(anyhow::Error),
}

impl From<DeterministicHostError> for HostExportError {
    fn from(value: DeterministicHostError) -> Self {
        match value {
            // Until we are confident on the gas numbers, gas errors are not deterministic
            DeterministicHostError::Gas(e) => HostExportError::Unknown(e),
            DeterministicHostError::Other(e) => HostExportError::Deterministic(e),
        }
    }
}

pub const HEADER_SIZE: usize = 20;

pub fn padding_to_16(content_length: usize) -> usize {
    (16 - (HEADER_SIZE + content_length) % 16) % 16
}
