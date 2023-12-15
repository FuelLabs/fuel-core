use ethabi;
use semver::Version;

use graph::{
    data::store,
    runtime::{
        gas::GasCounter, AscHeap, AscIndexId, AscType, AscValue, HostExportError,
        IndexForAscTypeId, ToAscObj,
    },
};
use graph::{prelude::serde_json, runtime::DeterministicHostError};
use graph::{prelude::slog, runtime::AscPtr};
use graph_runtime_derive::AscType;

use crate::asc_abi::{v0_0_4, v0_0_5};

///! Rust types that have with a direct correspondence to an Asc class,
///! with their `AscType` implementations.

/// Wrapper of ArrayBuffer for multiple AssemblyScript versions.
/// It just delegates its method calls to the correct mappings apiVersion.
pub enum ArrayBuffer {
    ApiVersion0_0_4(v0_0_4::ArrayBuffer),
    ApiVersion0_0_5(v0_0_5::ArrayBuffer),
}

impl ArrayBuffer {
    pub(crate) fn new<T: AscType>(
        values: &[T],
        api_version: Version,
    ) -> Result<Self, DeterministicHostError> {
        match api_version {
            version if version <= Version::new(0, 0, 4) => {
                Ok(Self::ApiVersion0_0_4(v0_0_4::ArrayBuffer::new(values)?))
            }
            _ => Ok(Self::ApiVersion0_0_5(v0_0_5::ArrayBuffer::new(values)?)),
        }
    }
}

impl AscType for ArrayBuffer {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        match self {
            Self::ApiVersion0_0_4(a) => a.to_asc_bytes(),
            Self::ApiVersion0_0_5(a) => a.to_asc_bytes(),
        }
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        match api_version {
            version if *version <= Version::new(0, 0, 4) => Ok(Self::ApiVersion0_0_4(
                v0_0_4::ArrayBuffer::from_asc_bytes(asc_obj, api_version)?,
            )),
            _ => Ok(Self::ApiVersion0_0_5(v0_0_5::ArrayBuffer::from_asc_bytes(
                asc_obj,
                api_version,
            )?)),
        }
    }

    fn asc_size<H: AscHeap + ?Sized>(
        ptr: AscPtr<Self>,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<u32, DeterministicHostError> {
        v0_0_4::ArrayBuffer::asc_size(AscPtr::new(ptr.wasm_ptr()), heap, gas)
    }

    fn content_len(&self, asc_bytes: &[u8]) -> usize {
        match self {
            Self::ApiVersion0_0_5(a) => a.content_len(asc_bytes),
            _ => unreachable!("Only called for apiVersion >=0.0.5"),
        }
    }
}

impl AscIndexId for ArrayBuffer {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayBuffer;
}

/// Wrapper of TypedArray for multiple AssemblyScript versions.
/// It just delegates its method calls to the correct mappings apiVersion.
pub enum TypedArray<T> {
    ApiVersion0_0_4(v0_0_4::TypedArray<T>),
    ApiVersion0_0_5(v0_0_5::TypedArray<T>),
}

impl<T: AscValue> TypedArray<T> {
    pub fn new<H: AscHeap + ?Sized>(
        content: &[T],
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<Self, HostExportError> {
        match heap.api_version() {
            version if version <= Version::new(0, 0, 4) => Ok(Self::ApiVersion0_0_4(
                v0_0_4::TypedArray::new(content, heap, gas)?,
            )),
            _ => Ok(Self::ApiVersion0_0_5(v0_0_5::TypedArray::new(
                content, heap, gas,
            )?)),
        }
    }

    pub fn to_vec<H: AscHeap + ?Sized>(
        &self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<Vec<T>, DeterministicHostError> {
        match self {
            Self::ApiVersion0_0_4(t) => t.to_vec(heap, gas),
            Self::ApiVersion0_0_5(t) => t.to_vec(heap, gas),
        }
    }
}

impl<T> AscType for TypedArray<T> {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        match self {
            Self::ApiVersion0_0_4(t) => t.to_asc_bytes(),
            Self::ApiVersion0_0_5(t) => t.to_asc_bytes(),
        }
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        match api_version {
            version if *version <= Version::new(0, 0, 4) => Ok(Self::ApiVersion0_0_4(
                v0_0_4::TypedArray::from_asc_bytes(asc_obj, api_version)?,
            )),
            _ => Ok(Self::ApiVersion0_0_5(v0_0_5::TypedArray::from_asc_bytes(
                asc_obj,
                api_version,
            )?)),
        }
    }
}

pub struct Bytes<'a>(pub &'a Vec<u8>);

pub type Uint8Array = TypedArray<u8>;
impl ToAscObj<Uint8Array> for Bytes<'_> {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<Uint8Array, HostExportError> {
        self.0.to_asc_obj(heap, gas)
    }
}

impl AscIndexId for TypedArray<i8> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Int8Array;
}

impl AscIndexId for TypedArray<i16> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Int16Array;
}

impl AscIndexId for TypedArray<i32> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Int32Array;
}

impl AscIndexId for TypedArray<i64> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Int64Array;
}

impl AscIndexId for TypedArray<u8> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Uint8Array;
}

impl AscIndexId for TypedArray<u16> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Uint16Array;
}

impl AscIndexId for TypedArray<u32> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Uint32Array;
}

impl AscIndexId for TypedArray<u64> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Uint64Array;
}

impl AscIndexId for TypedArray<f32> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Float32Array;
}

impl AscIndexId for TypedArray<f64> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::Float64Array;
}

/// Wrapper of String for multiple AssemblyScript versions.
/// It just delegates its method calls to the correct mappings apiVersion.
pub enum AscString {
    ApiVersion0_0_4(v0_0_4::AscString),
    ApiVersion0_0_5(v0_0_5::AscString),
}

impl AscString {
    pub fn new(content: &[u16], api_version: Version) -> Result<Self, DeterministicHostError> {
        match api_version {
            version if version <= Version::new(0, 0, 4) => {
                Ok(Self::ApiVersion0_0_4(v0_0_4::AscString::new(content)?))
            }
            _ => Ok(Self::ApiVersion0_0_5(v0_0_5::AscString::new(content)?)),
        }
    }

    pub fn content(&self) -> &[u16] {
        match self {
            Self::ApiVersion0_0_4(s) => &s.content,
            Self::ApiVersion0_0_5(s) => &s.content,
        }
    }
}

impl AscIndexId for AscString {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::String;
}

impl AscType for AscString {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        match self {
            Self::ApiVersion0_0_4(s) => s.to_asc_bytes(),
            Self::ApiVersion0_0_5(s) => s.to_asc_bytes(),
        }
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        match api_version {
            version if *version <= Version::new(0, 0, 4) => Ok(Self::ApiVersion0_0_4(
                v0_0_4::AscString::from_asc_bytes(asc_obj, api_version)?,
            )),
            _ => Ok(Self::ApiVersion0_0_5(v0_0_5::AscString::from_asc_bytes(
                asc_obj,
                api_version,
            )?)),
        }
    }

    fn asc_size<H: AscHeap + ?Sized>(
        ptr: AscPtr<Self>,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<u32, DeterministicHostError> {
        v0_0_4::AscString::asc_size(AscPtr::new(ptr.wasm_ptr()), heap, gas)
    }

    fn content_len(&self, asc_bytes: &[u8]) -> usize {
        match self {
            Self::ApiVersion0_0_5(s) => s.content_len(asc_bytes),
            _ => unreachable!("Only called for apiVersion >=0.0.5"),
        }
    }
}

/// Wrapper of Array for multiple AssemblyScript versions.
/// It just delegates its method calls to the correct mappings apiVersion.
pub enum Array<T> {
    ApiVersion0_0_4(v0_0_4::Array<T>),
    ApiVersion0_0_5(v0_0_5::Array<T>),
}

impl<T: AscValue> Array<T> {
    pub fn new<H: AscHeap + ?Sized>(
        content: &[T],
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<Self, HostExportError> {
        match heap.api_version() {
            version if version <= Version::new(0, 0, 4) => Ok(Self::ApiVersion0_0_4(
                v0_0_4::Array::new(content, heap, gas)?,
            )),
            _ => Ok(Self::ApiVersion0_0_5(v0_0_5::Array::new(
                content, heap, gas,
            )?)),
        }
    }

    pub(crate) fn to_vec<H: AscHeap + ?Sized>(
        &self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<Vec<T>, DeterministicHostError> {
        match self {
            Self::ApiVersion0_0_4(a) => a.to_vec(heap, gas),
            Self::ApiVersion0_0_5(a) => a.to_vec(heap, gas),
        }
    }
}

impl<T> AscType for Array<T> {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        match self {
            Self::ApiVersion0_0_4(a) => a.to_asc_bytes(),
            Self::ApiVersion0_0_5(a) => a.to_asc_bytes(),
        }
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        match api_version {
            version if *version <= Version::new(0, 0, 4) => Ok(Self::ApiVersion0_0_4(
                v0_0_4::Array::from_asc_bytes(asc_obj, api_version)?,
            )),
            _ => Ok(Self::ApiVersion0_0_5(v0_0_5::Array::from_asc_bytes(
                asc_obj,
                api_version,
            )?)),
        }
    }
}

impl AscIndexId for Array<bool> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayBool;
}

impl AscIndexId for Array<Uint8Array> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayUint8Array;
}

impl AscIndexId for Array<AscPtr<AscEnum<EthereumValueKind>>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayEthereumValue;
}

impl AscIndexId for Array<AscPtr<AscEnum<StoreValueKind>>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayStoreValue;
}

impl AscIndexId for Array<AscPtr<AscEnum<JsonValueKind>>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayJsonValue;
}

impl AscIndexId for Array<AscPtr<AscString>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayString;
}

impl AscIndexId for Array<AscPtr<AscTypedMapEntry<AscString, AscEnum<JsonValueKind>>>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId =
        IndexForAscTypeId::ArrayTypedMapEntryStringJsonValue;
}

impl AscIndexId for Array<AscPtr<AscTypedMapEntry<AscString, AscEnum<StoreValueKind>>>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId =
        IndexForAscTypeId::ArrayTypedMapEntryStringStoreValue;
}

impl AscIndexId for Array<u8> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayU8;
}

impl AscIndexId for Array<u16> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayU16;
}

impl AscIndexId for Array<u32> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayU32;
}

impl AscIndexId for Array<u64> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayU64;
}

impl AscIndexId for Array<i8> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayI8;
}

impl AscIndexId for Array<i16> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayI16;
}

impl AscIndexId for Array<i32> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayI32;
}

impl AscIndexId for Array<i64> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayI64;
}

impl AscIndexId for Array<f32> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayF32;
}

impl AscIndexId for Array<f64> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayF64;
}

impl AscIndexId for Array<AscPtr<AscBigDecimal>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayBigDecimal;
}

/// Represents any `AscValue` since they all fit in 64 bits.
#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct EnumPayload(pub u64);

impl AscType for EnumPayload {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        Ok(EnumPayload(u64::from_asc_bytes(asc_obj, api_version)?))
    }
}

impl From<EnumPayload> for i32 {
    fn from(payload: EnumPayload) -> i32 {
        payload.0 as i32
    }
}

impl From<EnumPayload> for f64 {
    fn from(payload: EnumPayload) -> f64 {
        f64::from_bits(payload.0)
    }
}

impl From<EnumPayload> for i64 {
    fn from(payload: EnumPayload) -> i64 {
        payload.0 as i64
    }
}

impl From<EnumPayload> for bool {
    fn from(payload: EnumPayload) -> bool {
        payload.0 != 0
    }
}

impl From<i32> for EnumPayload {
    fn from(x: i32) -> EnumPayload {
        EnumPayload(x as u64)
    }
}

impl From<f64> for EnumPayload {
    fn from(x: f64) -> EnumPayload {
        EnumPayload(x.to_bits())
    }
}

impl From<bool> for EnumPayload {
    fn from(b: bool) -> EnumPayload {
        EnumPayload(b.into())
    }
}

impl From<i64> for EnumPayload {
    fn from(x: i64) -> EnumPayload {
        EnumPayload(x as u64)
    }
}

impl<C> From<EnumPayload> for AscPtr<C> {
    fn from(payload: EnumPayload) -> Self {
        AscPtr::new(payload.0 as u32)
    }
}

impl<C> From<AscPtr<C>> for EnumPayload {
    fn from(x: AscPtr<C>) -> EnumPayload {
        EnumPayload(x.wasm_ptr() as u64)
    }
}

/// In Asc, we represent a Rust enum as a discriminant `kind: D`, which is an
/// Asc enum so in Rust it's a `#[repr(u32)]` enum, plus an arbitrary `AscValue`
/// payload.
#[repr(C)]
#[derive(AscType)]
pub struct AscEnum<D: AscValue> {
    pub kind: D,
    pub _padding: u32, // Make padding explicit.
    pub payload: EnumPayload,
}

impl AscIndexId for AscEnum<EthereumValueKind> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::EthereumValue;
}

impl AscIndexId for AscEnum<StoreValueKind> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::StoreValue;
}

impl AscIndexId for AscEnum<JsonValueKind> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::JsonValue;
}

pub type AscEnumArray<D> = AscPtr<Array<AscPtr<AscEnum<D>>>>;

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub enum EthereumValueKind {
    Address,
    FixedBytes,
    Bytes,
    Int,
    Uint,
    Bool,
    String,
    FixedArray,
    Array,
    Tuple,
}

impl EthereumValueKind {
    pub(crate) fn get_kind(token: &ethabi::Token) -> Self {
        match token {
            ethabi::Token::Address(_) => EthereumValueKind::Address,
            ethabi::Token::FixedBytes(_) => EthereumValueKind::FixedBytes,
            ethabi::Token::Bytes(_) => EthereumValueKind::Bytes,
            ethabi::Token::Int(_) => EthereumValueKind::Int,
            ethabi::Token::Uint(_) => EthereumValueKind::Uint,
            ethabi::Token::Bool(_) => EthereumValueKind::Bool,
            ethabi::Token::String(_) => EthereumValueKind::String,
            ethabi::Token::FixedArray(_) => EthereumValueKind::FixedArray,
            ethabi::Token::Array(_) => EthereumValueKind::Array,
            ethabi::Token::Tuple(_) => EthereumValueKind::Tuple,
        }
    }
}

impl Default for EthereumValueKind {
    fn default() -> Self {
        EthereumValueKind::Address
    }
}

impl AscValue for EthereumValueKind {}

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub enum StoreValueKind {
    String,
    Int,
    BigDecimal,
    Bool,
    Array,
    Null,
    Bytes,
    BigInt,
    Int8,
}

impl StoreValueKind {
    pub(crate) fn get_kind(value: &store::Value) -> StoreValueKind {
        use self::store::Value;

        match value {
            Value::String(_) => StoreValueKind::String,
            Value::Int(_) => StoreValueKind::Int,
            Value::Int8(_) => StoreValueKind::Int8,
            Value::BigDecimal(_) => StoreValueKind::BigDecimal,
            Value::Bool(_) => StoreValueKind::Bool,
            Value::List(_) => StoreValueKind::Array,
            Value::Null => StoreValueKind::Null,
            Value::Bytes(_) => StoreValueKind::Bytes,
            Value::BigInt(_) => StoreValueKind::BigInt,
        }
    }
}

impl Default for StoreValueKind {
    fn default() -> Self {
        StoreValueKind::Null
    }
}

impl AscValue for StoreValueKind {}

/// Big ints are represented using signed number representation. Note: This differs
/// from how U256 and U128 are represented (they use two's complement). So whenever
/// we convert between them, we need to make sure we handle signed and unsigned
/// cases correctly.
pub type AscBigInt = Uint8Array;

pub type AscAddress = Uint8Array;
pub type AscH160 = Uint8Array;

#[repr(C)]
#[derive(AscType)]
pub struct AscTypedMapEntry<K, V> {
    pub key: AscPtr<K>,
    pub value: AscPtr<V>,
}

impl AscIndexId for AscTypedMapEntry<AscString, AscEnum<StoreValueKind>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::TypedMapEntryStringStoreValue;
}

impl AscIndexId for AscTypedMapEntry<AscString, AscEnum<JsonValueKind>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::TypedMapEntryStringJsonValue;
}

pub(crate) type AscTypedMapEntryArray<K, V> = Array<AscPtr<AscTypedMapEntry<K, V>>>;

#[repr(C)]
#[derive(AscType)]
pub struct AscTypedMap<K, V> {
    pub entries: AscPtr<AscTypedMapEntryArray<K, V>>,
}

impl AscIndexId for AscTypedMap<AscString, AscEnum<StoreValueKind>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::TypedMapStringStoreValue;
}

impl AscIndexId for Array<AscPtr<AscEntity>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ArrayTypedMapStringStoreValue;
}

impl AscIndexId for AscTypedMap<AscString, AscEnum<JsonValueKind>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::TypedMapStringJsonValue;
}

impl AscIndexId for AscTypedMap<AscString, AscTypedMap<AscString, AscEnum<JsonValueKind>>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId =
        IndexForAscTypeId::TypedMapStringTypedMapStringJsonValue;
}

pub type AscEntity = AscTypedMap<AscString, AscEnum<StoreValueKind>>;
pub(crate) type AscJson = AscTypedMap<AscString, AscEnum<JsonValueKind>>;

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub enum JsonValueKind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

impl Default for JsonValueKind {
    fn default() -> Self {
        JsonValueKind::Null
    }
}

impl AscValue for JsonValueKind {}

impl JsonValueKind {
    pub(crate) fn get_kind(token: &serde_json::Value) -> Self {
        use serde_json::Value;

        match token {
            Value::Null => JsonValueKind::Null,
            Value::Bool(_) => JsonValueKind::Bool,
            Value::Number(_) => JsonValueKind::Number,
            Value::String(_) => JsonValueKind::String,
            Value::Array(_) => JsonValueKind::Array,
            Value::Object(_) => JsonValueKind::Object,
        }
    }
}

#[repr(C)]
#[derive(AscType)]
pub struct AscBigDecimal {
    pub digits: AscPtr<AscBigInt>,

    // Decimal exponent. This is the opposite of `scale` in rust BigDecimal.
    pub exp: AscPtr<AscBigInt>,
}

impl AscIndexId for AscBigDecimal {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::BigDecimal;
}

#[repr(u32)]
pub(crate) enum LogLevel {
    Critical,
    Error,
    Warning,
    Info,
    Debug,
}

impl From<LogLevel> for slog::Level {
    fn from(level: LogLevel) -> slog::Level {
        match level {
            LogLevel::Critical => slog::Level::Critical,
            LogLevel::Error => slog::Level::Error,
            LogLevel::Warning => slog::Level::Warning,
            LogLevel::Info => slog::Level::Info,
            LogLevel::Debug => slog::Level::Debug,
        }
    }
}

#[repr(C)]
#[derive(AscType)]
pub struct AscResult<V: AscValue, E: AscValue> {
    pub value: AscPtr<AscWrapped<V>>,
    pub error: AscPtr<AscWrapped<E>>,
}

impl AscIndexId for AscResult<AscPtr<AscJson>, bool> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId =
        IndexForAscTypeId::ResultTypedMapStringJsonValueBool;
}

impl AscIndexId for AscResult<AscPtr<AscEnum<JsonValueKind>>, bool> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::ResultJsonValueBool;
}

#[repr(C)]
#[derive(AscType, Copy, Clone)]
pub struct AscWrapped<V: AscValue> {
    pub inner: V,
}

impl AscIndexId for AscWrapped<AscPtr<AscJson>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::WrappedTypedMapStringJsonValue;
}

impl AscIndexId for AscWrapped<bool> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::WrappedBool;
}

impl AscIndexId for AscWrapped<AscPtr<AscEnum<JsonValueKind>>> {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::WrappedJsonValue;
}
