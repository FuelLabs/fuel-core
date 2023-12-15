use graph::components::metrics::gas::GasMetrics;
use graph::data::store::{scalar, Id, IdType};
use graph::data::subgraph::*;
use graph::data::value::Word;
use graph::prelude::web3::types::U256;
use graph::runtime::gas::GasCounter;
use graph::runtime::{AscIndexId, AscType, HostExportError};
use graph::runtime::{AscPtr, ToAscObj};
use graph::schema::{EntityType, InputSchema};
use graph::{components::store::*, ipfs_client::IpfsClient};
use graph::{entity, prelude::*};
use graph_chain_ethereum::{Chain, DataSource};
use graph_runtime_wasm::asc_abi::class::{Array, AscBigInt, AscEntity, AscString, Uint8Array};
use graph_runtime_wasm::{
    host_exports, ExperimentalFeatures, MappingContext, ValidModule, WasmInstance,
};

use semver::Version;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use test_store::{LOGGER, STORE};
use web3::types::H160;

use crate::common::{mock_context, mock_data_source};

mod abi;

pub const API_VERSION_0_0_4: Version = Version::new(0, 0, 4);
pub const API_VERSION_0_0_5: Version = Version::new(0, 0, 5);

pub fn wasm_file_path(wasm_file: &str, api_version: Version) -> String {
    format!(
        "wasm_test/api_version_{}_{}_{}/{}",
        api_version.major, api_version.minor, api_version.patch, wasm_file
    )
}

fn subgraph_id_with_api_version(subgraph_id: &str, api_version: Version) -> String {
    format!(
        "{}_{}_{}_{}",
        subgraph_id, api_version.major, api_version.minor, api_version.patch
    )
}

async fn test_valid_module_and_store(
    subgraph_id: &str,
    data_source: DataSource,
    api_version: Version,
) -> (
    WasmInstance<Chain>,
    Arc<impl SubgraphStore>,
    DeploymentLocator,
) {
    test_valid_module_and_store_with_timeout(subgraph_id, data_source, api_version, None).await
}

async fn test_valid_module_and_store_with_timeout(
    subgraph_id: &str,
    data_source: DataSource,
    api_version: Version,
    timeout: Option<Duration>,
) -> (
    WasmInstance<Chain>,
    Arc<impl SubgraphStore>,
    DeploymentLocator,
) {
    let logger = Logger::root(slog::Discard, o!());
    let subgraph_id_with_api_version =
        subgraph_id_with_api_version(subgraph_id, api_version.clone());

    let store = STORE.clone();
    let metrics_registry = Arc::new(MetricsRegistry::mock());
    let deployment_id = DeploymentHash::new(&subgraph_id_with_api_version).unwrap();
    let deployment = test_store::create_test_subgraph(
        &deployment_id,
        "type User @entity {
            id: ID!,
            name: String,
            count: BigInt,
        }

        type Thing @entity {
            id: ID!,
            value: String,
            extra: String
        }",
    )
    .await;
    let stopwatch_metrics = StopwatchMetrics::new(
        logger.clone(),
        deployment_id.clone(),
        "test",
        metrics_registry.clone(),
        "test_shard".to_string(),
    );

    let gas_metrics = GasMetrics::new(deployment_id.clone(), metrics_registry.clone());

    let host_metrics = Arc::new(HostMetrics::new(
        metrics_registry,
        deployment_id.as_str(),
        stopwatch_metrics,
        gas_metrics,
    ));

    let experimental_features = ExperimentalFeatures {
        allow_non_deterministic_ipfs: true,
    };

    let module = WasmInstance::from_valid_module_with_ctx(
        Arc::new(ValidModule::new(&logger, data_source.mapping.runtime.as_ref()).unwrap()),
        mock_context(
            deployment.clone(),
            data_source,
            store.subgraph_store(),
            api_version,
        ),
        host_metrics,
        timeout,
        experimental_features,
    )
    .unwrap();

    (module, store.subgraph_store(), deployment)
}

pub async fn test_module(
    subgraph_id: &str,
    data_source: DataSource,
    api_version: Version,
) -> WasmInstance<Chain> {
    test_valid_module_and_store(subgraph_id, data_source, api_version)
        .await
        .0
}

// A test module using the latest API version
pub async fn test_module_latest(subgraph_id: &str, wasm_file: &str) -> WasmInstance<Chain> {
    let version = ENV_VARS.mappings.max_api_version.clone();
    let ds = mock_data_source(
        &wasm_file_path(wasm_file, API_VERSION_0_0_5),
        version.clone(),
    );
    test_valid_module_and_store(subgraph_id, ds, version)
        .await
        .0
}

pub trait WasmInstanceExt {
    fn invoke_export0_void(&self, f: &str) -> Result<(), wasmtime::Trap>;
    fn invoke_export1_val_void<V: wasmtime::WasmTy>(
        &self,
        f: &str,
        v: V,
    ) -> Result<(), wasmtime::Trap>;
    fn invoke_export0<R>(&self, f: &str) -> AscPtr<R>;
    fn invoke_export1<C, T, R>(&mut self, f: &str, arg: &T) -> AscPtr<R>
    where
        C: AscType + AscIndexId,
        T: ToAscObj<C> + ?Sized;
    fn invoke_export2<C1, T1, T2, C2, R>(&mut self, f: &str, arg0: &T1, arg1: &T2) -> AscPtr<R>
    where
        C1: AscType + AscIndexId,
        C2: AscType + AscIndexId,
        T1: ToAscObj<C1> + ?Sized,
        T2: ToAscObj<C2> + ?Sized;
    fn invoke_export2_void<C1, T1, T2, C2>(
        &mut self,
        f: &str,
        arg0: &T1,
        arg1: &T2,
    ) -> Result<(), wasmtime::Trap>
    where
        C1: AscType + AscIndexId,
        C2: AscType + AscIndexId,
        T1: ToAscObj<C1> + ?Sized,
        T2: ToAscObj<C2> + ?Sized;
    fn invoke_export0_val<V: wasmtime::WasmTy>(&mut self, func: &str) -> V;
    fn invoke_export1_val<V: wasmtime::WasmTy, C, T>(&mut self, func: &str, v: &T) -> V
    where
        C: AscType + AscIndexId,
        T: ToAscObj<C> + ?Sized;
    fn takes_ptr_returns_ptr<C, R>(&self, f: &str, arg: AscPtr<C>) -> AscPtr<R>;
    fn takes_val_returns_ptr<P>(&mut self, fn_name: &str, val: impl wasmtime::WasmTy) -> AscPtr<P>;
}

impl WasmInstanceExt for WasmInstance<Chain> {
    fn invoke_export0_void(&self, f: &str) -> Result<(), wasmtime::Trap> {
        let func = self.get_func(f).typed().unwrap().clone();
        func.call(())
    }

    fn invoke_export0<R>(&self, f: &str) -> AscPtr<R> {
        let func = self.get_func(f).typed().unwrap().clone();
        let ptr: u32 = func.call(()).unwrap();
        ptr.into()
    }

    fn takes_ptr_returns_ptr<C, R>(&self, f: &str, arg: AscPtr<C>) -> AscPtr<R> {
        let func = self.get_func(f).typed().unwrap().clone();
        let ptr: u32 = func.call(arg.wasm_ptr()).unwrap();
        ptr.into()
    }

    fn invoke_export1<C, T, R>(&mut self, f: &str, arg: &T) -> AscPtr<R>
    where
        C: AscType + AscIndexId,
        T: ToAscObj<C> + ?Sized,
    {
        let func = self.get_func(f).typed().unwrap().clone();
        let ptr = self.asc_new(arg).unwrap();
        let ptr: u32 = func.call(ptr.wasm_ptr()).unwrap();
        ptr.into()
    }

    fn invoke_export1_val_void<V: wasmtime::WasmTy>(
        &self,
        f: &str,
        v: V,
    ) -> Result<(), wasmtime::Trap> {
        let func = self.get_func(f).typed().unwrap().clone();
        func.call(v)?;
        Ok(())
    }

    fn invoke_export2<C1, T1, T2, C2, R>(&mut self, f: &str, arg0: &T1, arg1: &T2) -> AscPtr<R>
    where
        C1: AscType + AscIndexId,
        C2: AscType + AscIndexId,
        T1: ToAscObj<C1> + ?Sized,
        T2: ToAscObj<C2> + ?Sized,
    {
        let func = self.get_func(f).typed().unwrap().clone();
        let arg0 = self.asc_new(arg0).unwrap();
        let arg1 = self.asc_new(arg1).unwrap();
        let ptr: u32 = func.call((arg0.wasm_ptr(), arg1.wasm_ptr())).unwrap();
        ptr.into()
    }

    fn invoke_export2_void<C1, T1, T2, C2>(
        &mut self,
        f: &str,
        arg0: &T1,
        arg1: &T2,
    ) -> Result<(), wasmtime::Trap>
    where
        C1: AscType + AscIndexId,
        C2: AscType + AscIndexId,
        T1: ToAscObj<C1> + ?Sized,
        T2: ToAscObj<C2> + ?Sized,
    {
        let func = self.get_func(f).typed().unwrap().clone();
        let arg0 = self.asc_new(arg0).unwrap();
        let arg1 = self.asc_new(arg1).unwrap();
        func.call((arg0.wasm_ptr(), arg1.wasm_ptr()))
    }

    fn invoke_export0_val<V: wasmtime::WasmTy>(&mut self, func: &str) -> V {
        let func = self.get_func(func).typed().unwrap().clone();
        func.call(()).unwrap()
    }

    fn invoke_export1_val<V: wasmtime::WasmTy, C, T>(&mut self, func: &str, v: &T) -> V
    where
        C: AscType + AscIndexId,
        T: ToAscObj<C> + ?Sized,
    {
        let func = self.get_func(func).typed().unwrap().clone();
        let ptr = self.asc_new(v).unwrap();
        func.call(ptr.wasm_ptr()).unwrap()
    }

    fn takes_val_returns_ptr<P>(&mut self, fn_name: &str, val: impl wasmtime::WasmTy) -> AscPtr<P> {
        let func = self.get_func(fn_name).typed().unwrap().clone();
        let ptr: u32 = func.call(val).unwrap();
        ptr.into()
    }
}

async fn test_json_conversions(api_version: Version, gas_used: u64) {
    let mut module = test_module(
        "jsonConversions",
        mock_data_source(
            &wasm_file_path("string_to_number.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    // test u64 conversion
    let number = 9223372036850770800;
    let converted: i64 = module.invoke_export1_val("testToU64", &number.to_string());
    assert_eq!(number, u64::from_le_bytes(converted.to_le_bytes()));

    // test i64 conversion
    let number = -9223372036850770800;
    let converted: i64 = module.invoke_export1_val("testToI64", &number.to_string());
    assert_eq!(number, converted);

    // test f64 conversion
    let number = -9223372036850770.92345034;
    let converted: f64 = module.invoke_export1_val("testToF64", &number.to_string());
    assert_eq!(number, converted);

    // test BigInt conversion
    let number = "-922337203685077092345034";
    let big_int_obj: AscPtr<AscBigInt> = module.invoke_export1("testToBigInt", number);
    let bytes: Vec<u8> = module.asc_get(big_int_obj).unwrap();

    assert_eq!(
        scalar::BigInt::from_str(number).unwrap(),
        scalar::BigInt::from_signed_bytes_le(&bytes).unwrap()
    );

    assert_eq!(module.gas_used(), gas_used);
}

#[tokio::test]
async fn json_conversions_v0_0_4() {
    test_json_conversions(API_VERSION_0_0_4, 52976429).await;
}

#[tokio::test]
async fn json_conversions_v0_0_5() {
    test_json_conversions(API_VERSION_0_0_5, 2289897).await;
}

async fn test_json_parsing(api_version: Version, gas_used: u64) {
    let mut module = test_module(
        "jsonParsing",
        mock_data_source(
            &wasm_file_path("json_parsing.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    // Parse valid JSON and get it back
    let s = "\"foo\""; // Valid because there are quotes around `foo`
    let bytes: &[u8] = s.as_ref();
    let return_value: AscPtr<AscString> = module.invoke_export1("handleJsonError", bytes);

    let output: String = module.asc_get(return_value).unwrap();
    assert_eq!(output, "OK: foo, ERROR: false");
    assert_eq!(module.gas_used(), gas_used);

    // Parse invalid JSON and handle the error gracefully
    let s = "foo"; // Invalid because there are no quotes around `foo`
    let bytes: &[u8] = s.as_ref();
    let return_value: AscPtr<AscString> = module.invoke_export1("handleJsonError", bytes);
    let output: String = module.asc_get(return_value).unwrap();
    assert_eq!(output, "ERROR: true");

    // Parse JSON that's too long and handle the error gracefully
    let s = format!("\"f{}\"", "o".repeat(10_000_000));
    let bytes: &[u8] = s.as_ref();
    let return_value: AscPtr<AscString> = module.invoke_export1("handleJsonError", bytes);

    let output: String = module.asc_get(return_value).unwrap();
    assert_eq!(output, "ERROR: true");
}

#[tokio::test]
async fn json_parsing_v0_0_4() {
    test_json_parsing(API_VERSION_0_0_4, 4373087).await;
}

#[tokio::test]
async fn json_parsing_v0_0_5() {
    test_json_parsing(API_VERSION_0_0_5, 5153540).await;
}

async fn test_ipfs_cat(api_version: Version) {
    // Ipfs host functions use `block_on` which must be called from a sync context,
    // so we replicate what we do `spawn_module`.
    let runtime = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let _runtime_guard = runtime.enter();

        let ipfs = IpfsClient::localhost();
        let hash = graph::block_on(ipfs.add("42".into())).unwrap().hash;

        let mut module = graph::block_on(test_module(
            "ipfsCat",
            mock_data_source(
                &wasm_file_path("ipfs_cat.wasm", api_version.clone()),
                api_version.clone(),
            ),
            api_version,
        ));
        let converted: AscPtr<AscString> = module.invoke_export1("ipfsCatString", &hash);
        let data: String = module.asc_get(converted).unwrap();
        assert_eq!(data, "42");
    })
    .join()
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn ipfs_cat_v0_0_4() {
    test_ipfs_cat(API_VERSION_0_0_4).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ipfs_cat_v0_0_5() {
    test_ipfs_cat(API_VERSION_0_0_5).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ipfs_block() {
    // Ipfs host functions use `block_on` which must be called from a sync context,
    // so we replicate what we do `spawn_module`.
    let runtime = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let _runtime_guard = runtime.enter();

        let ipfs = IpfsClient::localhost();
        let hash = graph::block_on(ipfs.add("42".into())).unwrap().hash;
        let mut module = graph::block_on(test_module(
            "ipfsBlock",
            mock_data_source(
                &wasm_file_path("ipfs_block.wasm", API_VERSION_0_0_5),
                API_VERSION_0_0_5,
            ),
            API_VERSION_0_0_5,
        ));
        let converted: AscPtr<AscString> = module.invoke_export1("ipfsBlockHex", &hash);
        let data: String = module.asc_get(converted).unwrap();
        assert_eq!(data, "0x0a080802120234321802");
    })
    .join()
    .unwrap();
}

// The user_data value we use with calls to ipfs_map
const USER_DATA: &str = "user_data";

fn make_thing(id: &str, value: &str) -> (String, EntityModification) {
    const DOCUMENT: &str = " type Thing @entity { id: String!, value: String!, extra: String }";
    lazy_static! {
        static ref SCHEMA: InputSchema = InputSchema::raw(DOCUMENT, "doesntmatter");
        static ref THING_TYPE: EntityType = SCHEMA.entity_type("Thing").unwrap();
    }
    let data = entity! { SCHEMA => id: id, value: value, extra: USER_DATA };
    let key = THING_TYPE.parse_key(id).unwrap();
    (
        format!("{{ \"id\": \"{}\", \"value\": \"{}\"}}", id, value),
        EntityModification::insert(key, data, 0),
    )
}

const BAD_IPFS_HASH: &str = "bad-ipfs-hash";

async fn run_ipfs_map(
    ipfs: IpfsClient,
    subgraph_id: &'static str,
    json_string: String,
    api_version: Version,
) -> Result<Vec<EntityModification>, anyhow::Error> {
    let hash = if json_string == BAD_IPFS_HASH {
        "Qm".to_string()
    } else {
        ipfs.add(json_string.into()).await.unwrap().hash
    };

    // Ipfs host functions use `block_on` which must be called from a sync context,
    // so we replicate what we do `spawn_module`.
    let runtime = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let _runtime_guard = runtime.enter();

        let (mut module, _, _) = graph::block_on(test_valid_module_and_store(
            subgraph_id,
            mock_data_source(
                &wasm_file_path("ipfs_map.wasm", api_version.clone()),
                api_version.clone(),
            ),
            api_version,
        ));

        let value = module.asc_new(&hash).unwrap();
        let user_data = module.asc_new(USER_DATA).unwrap();

        // Invoke the callback
        let func = module.get_func("ipfsMap").typed().unwrap().clone();
        func.call((value.wasm_ptr(), user_data.wasm_ptr()))?;
        let mut mods = module
            .take_ctx()
            .ctx
            .state
            .entity_cache
            .as_modifications(0)?
            .modifications;

        // Bring the modifications into a predictable order (by entity_id)
        mods.sort_by(|a, b| a.key().entity_id.partial_cmp(&b.key().entity_id).unwrap());
        Ok(mods)
    })
    .join()
    .unwrap()
}

async fn test_ipfs_map(api_version: Version, json_error_msg: &str) {
    let ipfs = IpfsClient::localhost();
    let subgraph_id = "ipfsMap";

    // Try it with two valid objects
    let (str1, thing1) = make_thing("one", "eins");
    let (str2, thing2) = make_thing("two", "zwei");
    let ops = run_ipfs_map(
        ipfs.clone(),
        subgraph_id,
        format!("{}\n{}", str1, str2),
        api_version.clone(),
    )
    .await
    .expect("call failed");
    let expected = vec![thing1, thing2];
    assert_eq!(expected, ops);

    // Valid JSON, but not what the callback expected; it will
    // fail on an assertion
    let err = run_ipfs_map(
        ipfs.clone(),
        subgraph_id,
        format!("{}\n[1,2]", str1),
        api_version.clone(),
    )
    .await
    .unwrap_err();
    assert!(
        format!("{:#}", err).contains("JSON value is not an object."),
        "{:#}",
        err
    );

    // Malformed JSON
    let errmsg = run_ipfs_map(
        ipfs.clone(),
        subgraph_id,
        format!("{}\n[", str1),
        api_version.clone(),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(errmsg.contains("EOF while parsing a list"));

    // Empty input
    let ops = run_ipfs_map(
        ipfs.clone(),
        subgraph_id,
        "".to_string(),
        api_version.clone(),
    )
    .await
    .expect("call failed for emoty string");
    assert_eq!(0, ops.len());

    // Missing entry in the JSON object
    let errmsg = format!(
        "{:#}",
        run_ipfs_map(
            ipfs.clone(),
            subgraph_id,
            "{\"value\": \"drei\"}".to_string(),
            api_version.clone(),
        )
        .await
        .unwrap_err()
    );
    assert!(errmsg.contains(json_error_msg));

    // Bad IPFS hash.
    let errmsg = run_ipfs_map(
        ipfs.clone(),
        subgraph_id,
        BAD_IPFS_HASH.to_string(),
        api_version.clone(),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(errmsg.contains("500 Internal Server Error"));
}

#[tokio::test(flavor = "multi_thread")]
async fn ipfs_map_v0_0_4() {
    test_ipfs_map(API_VERSION_0_0_4, "JSON value is not a string.").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ipfs_map_v0_0_5() {
    test_ipfs_map(API_VERSION_0_0_5, "'id' should not be null").await;
}

async fn test_ipfs_fail(api_version: Version) {
    let runtime = tokio::runtime::Handle::current();

    // Ipfs host functions use `block_on` which must be called from a sync context,
    // so we replicate what we do `spawn_module`.
    std::thread::spawn(move || {
        let _runtime_guard = runtime.enter();

        let mut module = graph::block_on(test_module(
            "ipfsFail",
            mock_data_source(
                &wasm_file_path("ipfs_cat.wasm", api_version.clone()),
                api_version.clone(),
            ),
            api_version,
        ));

        assert!(module
            .invoke_export1::<_, _, AscString>("ipfsCat", "invalid hash")
            .is_null());
    })
    .join()
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn ipfs_fail_v0_0_4() {
    test_ipfs_fail(API_VERSION_0_0_4).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ipfs_fail_v0_0_5() {
    test_ipfs_fail(API_VERSION_0_0_5).await;
}

async fn test_crypto_keccak256(api_version: Version) {
    let mut module = test_module(
        "cryptoKeccak256",
        mock_data_source(
            &wasm_file_path("crypto.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;
    let input: &[u8] = "eth".as_ref();

    let hash: AscPtr<Uint8Array> = module.invoke_export1("hash", input);
    let hash: Vec<u8> = module.asc_get(hash).unwrap();
    assert_eq!(
        hex::encode(hash),
        "4f5b812789fc606be1b3b16908db13fc7a9adf7ca72641f84d75b47069d3d7f0"
    );
}

#[tokio::test]
async fn crypto_keccak256_v0_0_4() {
    test_crypto_keccak256(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn crypto_keccak256_v0_0_5() {
    test_crypto_keccak256(API_VERSION_0_0_5).await;
}

async fn test_big_int_to_hex(api_version: Version, gas_used: u64) {
    let mut module = test_module(
        "BigIntToHex",
        mock_data_source(
            &wasm_file_path("big_int_to_hex.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    // Convert zero to hex
    let zero = BigInt::from_unsigned_u256(&U256::zero());
    let zero_hex_ptr: AscPtr<AscString> = module.invoke_export1("big_int_to_hex", &zero);
    let zero_hex_str: String = module.asc_get(zero_hex_ptr).unwrap();
    assert_eq!(zero_hex_str, "0x0");

    // Convert 1 to hex
    let one = BigInt::from_unsigned_u256(&U256::one());
    let one_hex_ptr: AscPtr<AscString> = module.invoke_export1("big_int_to_hex", &one);
    let one_hex_str: String = module.asc_get(one_hex_ptr).unwrap();
    assert_eq!(one_hex_str, "0x1");

    // Convert U256::max_value() to hex
    let u256_max = BigInt::from_unsigned_u256(&U256::max_value());
    let u256_max_hex_ptr: AscPtr<AscString> = module.invoke_export1("big_int_to_hex", &u256_max);
    let u256_max_hex_str: String = module.asc_get(u256_max_hex_ptr).unwrap();
    assert_eq!(
        u256_max_hex_str,
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
    );

    assert_eq!(module.gas_used(), gas_used);
}

#[tokio::test]
async fn test_big_int_size_limit() {
    let module = test_module(
        "BigIntSizeLimit",
        mock_data_source(
            &wasm_file_path("big_int_size_limit.wasm", API_VERSION_0_0_5),
            API_VERSION_0_0_5,
        ),
        API_VERSION_0_0_5,
    )
    .await;

    let len = BigInt::MAX_BITS / 8;
    module
        .invoke_export1_val_void("bigIntWithLength", len)
        .unwrap();

    let len = BigInt::MAX_BITS / 8 + 1;
    assert!(module
        .invoke_export1_val_void("bigIntWithLength", len)
        .unwrap_err()
        .to_string()
        .contains("BigInt is too big, total bits 435416 (max 435412)"));
}

#[tokio::test]
async fn big_int_to_hex_v0_0_4() {
    test_big_int_to_hex(API_VERSION_0_0_4, 53113760).await;
}

#[tokio::test]
async fn big_int_to_hex_v0_0_5() {
    test_big_int_to_hex(API_VERSION_0_0_5, 2858580).await;
}

async fn test_big_int_arithmetic(api_version: Version, gas_used: u64) {
    let mut module = test_module(
        "BigIntArithmetic",
        mock_data_source(
            &wasm_file_path("big_int_arithmetic.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    // 0 + 1 = 1
    let zero = BigInt::from(0);
    let one = BigInt::from(1);
    let result_ptr: AscPtr<AscBigInt> = module.invoke_export2("plus", &zero, &one);
    let result: BigInt = module.asc_get(result_ptr).unwrap();
    assert_eq!(result, BigInt::from(1));

    // 127 + 1 = 128
    let zero = BigInt::from(127);
    let one = BigInt::from(1);
    let result_ptr: AscPtr<AscBigInt> = module.invoke_export2("plus", &zero, &one);
    let result: BigInt = module.asc_get(result_ptr).unwrap();
    assert_eq!(result, BigInt::from(128));

    // 5 - 10 = -5
    let five = BigInt::from(5);
    let ten = BigInt::from(10);
    let result_ptr: AscPtr<AscBigInt> = module.invoke_export2("minus", &five, &ten);
    let result: BigInt = module.asc_get(result_ptr).unwrap();
    assert_eq!(result, BigInt::from(-5));

    // -20 * 5 = -100
    let minus_twenty = BigInt::from(-20);
    let five = BigInt::from(5);
    let result_ptr: AscPtr<AscBigInt> = module.invoke_export2("times", &minus_twenty, &five);
    let result: BigInt = module.asc_get(result_ptr).unwrap();
    assert_eq!(result, BigInt::from(-100));

    // 5 / 2 = 2
    let five = BigInt::from(5);
    let two = BigInt::from(2);
    let result_ptr: AscPtr<AscBigInt> = module.invoke_export2("dividedBy", &five, &two);
    let result: BigInt = module.asc_get(result_ptr).unwrap();
    assert_eq!(result, BigInt::from(2));

    // 5 % 2 = 1
    let five = BigInt::from(5);
    let two = BigInt::from(2);
    let result_ptr: AscPtr<AscBigInt> = module.invoke_export2("mod", &five, &two);
    let result: BigInt = module.asc_get(result_ptr).unwrap();
    assert_eq!(result, BigInt::from(1));

    assert_eq!(module.gas_used(), gas_used);
}

#[tokio::test]
async fn big_int_arithmetic_v0_0_4() {
    test_big_int_arithmetic(API_VERSION_0_0_4, 54962411).await;
}

#[tokio::test]
async fn big_int_arithmetic_v0_0_5() {
    test_big_int_arithmetic(API_VERSION_0_0_5, 7318364).await;
}

async fn test_abort(api_version: Version, error_msg: &str) {
    let module = test_module(
        "abort",
        mock_data_source(
            &wasm_file_path("abort.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;
    let res: Result<(), _> = module.get_func("abort").typed().unwrap().call(());
    assert!(res.unwrap_err().to_string().contains(error_msg));
}

#[tokio::test]
async fn abort_v0_0_4() {
    test_abort(
        API_VERSION_0_0_4,
        "line 6, column 2, with message: not true",
    )
    .await;
}

#[tokio::test]
async fn abort_v0_0_5() {
    test_abort(
        API_VERSION_0_0_5,
        "line 4, column 3, with message: not true",
    )
    .await;
}

async fn test_bytes_to_base58(api_version: Version, gas_used: u64) {
    let mut module = test_module(
        "bytesToBase58",
        mock_data_source(
            &wasm_file_path("bytes_to_base58.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;
    let bytes = hex::decode("12207D5A99F603F231D53A4F39D1521F98D2E8BB279CF29BEBFD0687DC98458E7F89")
        .unwrap();
    let result_ptr: AscPtr<AscString> = module.invoke_export1("bytes_to_base58", bytes.as_slice());
    let base58: String = module.asc_get(result_ptr).unwrap();

    assert_eq!(base58, "QmWmyoMoctfbAaiEs2G46gpeUmhqFRDW6KWo64y5r581Vz");
    assert_eq!(module.gas_used(), gas_used);
}

#[tokio::test]
async fn bytes_to_base58_v0_0_4() {
    test_bytes_to_base58(API_VERSION_0_0_4, 52301689).await;
}

#[tokio::test]
async fn bytes_to_base58_v0_0_5() {
    test_bytes_to_base58(API_VERSION_0_0_5, 1310019).await;
}

async fn test_data_source_create(api_version: Version, gas_used: u64) {
    // Test with a valid template
    let template = String::from("example template");
    let params = vec![String::from("0xc0a47dFe034B400B47bDaD5FecDa2621de6c4d95")];
    let result = run_data_source_create(
        template.clone(),
        params.clone(),
        api_version.clone(),
        gas_used,
    )
    .await
    .expect("unexpected error returned from dataSourceCreate");
    assert_eq!(result[0].params, params.clone());
    assert_eq!(result[0].template.name(), template);

    // Test with a template that doesn't exist
    let template = String::from("nonexistent template");
    let params = vec![String::from("0xc000000000000000000000000000000000000000")];
    match run_data_source_create(template.clone(), params.clone(), api_version, gas_used).await {
        Ok(_) => panic!("expected an error because the template does not exist"),
        Err(e) => assert!(e.to_string().contains(
            "Failed to create data source from name `nonexistent template`: \
             No template with this name in parent data source `example data source`. \
             Available names: example template."
        )),
    };
}

async fn run_data_source_create(
    name: String,
    params: Vec<String>,
    api_version: Version,
    gas_used: u64,
) -> Result<Vec<DataSourceTemplateInfo<Chain>>, wasmtime::Trap> {
    let mut module = test_module(
        "DataSourceCreate",
        mock_data_source(
            &wasm_file_path("data_source_create.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version.clone(),
    )
    .await;

    module.instance_ctx_mut().ctx.state.enter_handler();
    module.invoke_export2_void("dataSourceCreate", &name, &params)?;
    module.instance_ctx_mut().ctx.state.exit_handler();

    assert_eq!(module.gas_used(), gas_used);

    Ok(module.take_ctx().ctx.state.drain_created_data_sources())
}

#[tokio::test]
async fn data_source_create_v0_0_4() {
    test_data_source_create(API_VERSION_0_0_4, 152102833).await;
}

#[tokio::test]
async fn data_source_create_v0_0_5() {
    test_data_source_create(API_VERSION_0_0_5, 101450079).await;
}

async fn test_ens_name_by_hash(api_version: Version) {
    let mut module = test_module(
        "EnsNameByHash",
        mock_data_source(
            &wasm_file_path("ens_name_by_hash.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let hash = "0x7f0c1b04d1a4926f9c635a030eeb611d4c26e5e73291b32a1c7a4ac56935b5b3";
    let name = "dealdrafts";
    test_store::insert_ens_name(hash, name);
    let converted: AscPtr<AscString> = module.invoke_export1("nameByHash", hash);
    let data: String = module.asc_get(converted).unwrap();
    assert_eq!(data, name);

    assert!(module
        .invoke_export1::<_, _, AscString>("nameByHash", "impossible keccak hash")
        .is_null());
}

#[tokio::test]
async fn ens_name_by_hash_v0_0_4() {
    test_ens_name_by_hash(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn ens_name_by_hash_v0_0_5() {
    test_ens_name_by_hash(API_VERSION_0_0_5).await;
}

async fn test_entity_store(api_version: Version) {
    let (mut module, store, deployment) = test_valid_module_and_store(
        "entityStore",
        mock_data_source(
            &wasm_file_path("store.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let schema = store.input_schema(&deployment.hash).unwrap();

    let alex = entity! { schema => id: "alex", name: "Alex" };
    let steve = entity! { schema => id: "steve", name: "Steve" };
    let user_type = schema.entity_type("User").unwrap();
    test_store::insert_entities(
        &deployment,
        vec![(user_type.clone(), alex), (user_type, steve)],
    )
    .await
    .unwrap();

    let get_user = move |module: &mut WasmInstance<Chain>, id: &str| -> Option<Entity> {
        let entity_ptr: AscPtr<AscEntity> = module.invoke_export1("getUser", id);
        if entity_ptr.is_null() {
            None
        } else {
            Some(
                schema
                    .make_entity(
                        module
                            .asc_get::<HashMap<Word, Value>, _>(entity_ptr)
                            .unwrap(),
                    )
                    .unwrap(),
            )
        }
    };

    let load_and_set_user_name = |module: &mut WasmInstance<Chain>, id: &str, name: &str| {
        module
            .invoke_export2_void("loadAndSetUserName", id, name)
            .unwrap();
    };

    // store.get of a nonexistent user
    assert_eq!(None, get_user(&mut module, "herobrine"));
    // store.get of an existing user
    let steve = get_user(&mut module, "steve").unwrap();
    assert_eq!(Some(&Value::from("Steve")), steve.get("name"));

    // Load, set, save cycle for an existing entity
    load_and_set_user_name(&mut module, "steve", "Steve-O");

    // We need to empty the cache for the next test
    let writable = store
        .writable(LOGGER.clone(), deployment.id, Arc::new(Vec::new()))
        .await
        .unwrap();
    let cache = std::mem::replace(
        &mut module.instance_ctx_mut().ctx.state.entity_cache,
        EntityCache::new(Arc::new(writable.clone())),
    );
    let mut mods = cache.as_modifications(0).unwrap().modifications;
    assert_eq!(1, mods.len());
    match mods.pop().unwrap() {
        EntityModification::Overwrite { data, .. } => {
            assert_eq!(Some(&Value::from("steve")), data.get("id"));
            assert_eq!(Some(&Value::from("Steve-O")), data.get("name"));
        }
        _ => assert!(false, "expected Overwrite modification"),
    }

    // Load, set, save cycle for a new entity with fulltext API
    load_and_set_user_name(&mut module, "herobrine", "Brine-O");
    let mut fulltext_entities = BTreeMap::new();
    let mut fulltext_fields = BTreeMap::new();
    fulltext_fields.insert("name".to_string(), vec!["search".to_string()]);
    fulltext_entities.insert("User".to_string(), fulltext_fields);
    let mut mods = module
        .take_ctx()
        .ctx
        .state
        .entity_cache
        .as_modifications(0)
        .unwrap()
        .modifications;
    assert_eq!(1, mods.len());
    match mods.pop().unwrap() {
        EntityModification::Insert { data, .. } => {
            assert_eq!(Some(&Value::from("herobrine")), data.get("id"));
            assert_eq!(Some(&Value::from("Brine-O")), data.get("name"));
        }
        _ => assert!(false, "expected Insert modification"),
    };
}

#[tokio::test]
async fn entity_store_v0_0_4() {
    test_entity_store(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn entity_store_v0_0_5() {
    test_entity_store(API_VERSION_0_0_5).await;
}

fn test_detect_contract_calls(api_version: Version) {
    let data_source_without_calls = mock_data_source(
        &wasm_file_path("abi_store_value.wasm", api_version.clone()),
        api_version.clone(),
    );
    assert_eq!(
        data_source_without_calls
            .mapping
            .requires_archive()
            .unwrap(),
        false
    );

    let data_source_with_calls = mock_data_source(
        &wasm_file_path("contract_calls.wasm", api_version.clone()),
        api_version,
    );
    assert_eq!(
        data_source_with_calls.mapping.requires_archive().unwrap(),
        true
    );
}

#[tokio::test]
async fn detect_contract_calls_v0_0_4() {
    test_detect_contract_calls(API_VERSION_0_0_4);
}

#[tokio::test]
async fn detect_contract_calls_v0_0_5() {
    test_detect_contract_calls(API_VERSION_0_0_5);
}

async fn test_allocate_global(api_version: Version) {
    let module = test_module(
        "AllocateGlobal",
        mock_data_source(
            &wasm_file_path("allocate_global.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    // Assert globals can be allocated and don't break the heap
    module.invoke_export0_void("assert_global_works").unwrap();
}

#[tokio::test]
async fn allocate_global_v0_0_5() {
    // Only in apiVersion v0.0.5 because there's no issue in older versions.
    // The problem with the new one is related to the AS stub runtime `offset`
    // variable not being initialized (lazy) before we use it so this test checks
    // that it works (at the moment using __alloc call to force offset to be eagerly
    // evaluated).
    test_allocate_global(API_VERSION_0_0_5).await;
}

async fn test_null_ptr_read(api_version: Version) {
    let module = test_module(
        "NullPtrRead",
        mock_data_source(
            &wasm_file_path("null_ptr_read.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    module.invoke_export0_void("nullPtrRead").unwrap();
}

#[tokio::test]
#[should_panic(expected = "Tried to read AssemblyScript value that is 'null'")]
async fn null_ptr_read_0_0_5() {
    test_null_ptr_read(API_VERSION_0_0_5).await;
}

async fn test_safe_null_ptr_read(api_version: Version) {
    let module = test_module(
        "SafeNullPtrRead",
        mock_data_source(
            &wasm_file_path("null_ptr_read.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    module.invoke_export0_void("safeNullPtrRead").unwrap();
}

#[tokio::test]
#[should_panic(expected = "Failed to sum BigInts because left hand side is 'null'")]
async fn safe_null_ptr_read_0_0_5() {
    test_safe_null_ptr_read(API_VERSION_0_0_5).await;
}

#[ignore] // Ignored because of long run time in debug build.
#[tokio::test]
async fn test_array_blowup() {
    let module = test_module_latest("ArrayBlowup", "array_blowup.wasm").await;

    assert!(module
        .invoke_export0_void("arrayBlowup")
        .unwrap_err()
        .to_string()
        .contains("Gas limit exceeded. Used: 11286295575421"));
}

#[tokio::test]
async fn test_boolean() {
    let mut module = test_module_latest("boolean", "boolean.wasm").await;

    let true_: i32 = module.invoke_export0_val("testReturnTrue");
    assert_eq!(true_, 1);

    let false_: i32 = module.invoke_export0_val("testReturnFalse");
    assert_eq!(false_, 0);

    // non-zero values are true
    for x in (-10i32..10).filter(|&x| x != 0) {
        assert!(module.invoke_export1_val_void("testReceiveTrue", x).is_ok(),);
    }

    // zero is not true
    assert!(module
        .invoke_export1_val_void("testReceiveTrue", 0i32)
        .is_err());

    // zero is false
    assert!(module
        .invoke_export1_val_void("testReceiveFalse", 0i32)
        .is_ok());

    // non-zero values are not false
    for x in (-10i32..10).filter(|&x| x != 0) {
        assert!(module
            .invoke_export1_val_void("testReceiveFalse", x)
            .is_err());
    }
}

#[tokio::test]
async fn recursion_limit() {
    let module = test_module_latest("RecursionLimit", "recursion_limit.wasm").await;

    // An error about 'unknown key' means the entity was fully read with no stack overflow.
    module
        .invoke_export1_val_void("recursionLimit", 128)
        .unwrap_err()
        .to_string()
        .contains("Unknown key `foobar`");

    assert!(module
        .invoke_export1_val_void("recursionLimit", 129)
        .unwrap_err()
        .to_string()
        .contains("recursion limit reached"));
}

struct Host {
    ctx: MappingContext<Chain>,
    host_exports: host_exports::test_support::HostExports<Chain>,
    stopwatch: StopwatchMetrics,
    gas: GasCounter,
}

impl Host {
    async fn new(
        schema: &str,
        deployment_hash: &str,
        wasm_file: &str,
        api_version: Option<Version>,
    ) -> Host {
        let version = api_version.unwrap_or(ENV_VARS.mappings.max_api_version.clone());
        let wasm_file = wasm_file_path(wasm_file, API_VERSION_0_0_5);

        let ds = mock_data_source(&wasm_file, version.clone());

        let store = STORE.clone();
        let deployment = DeploymentHash::new(deployment_hash.to_string()).unwrap();
        let deployment = test_store::create_test_subgraph(&deployment, schema).await;
        let ctx = mock_context(deployment.clone(), ds, store.subgraph_store(), version);
        let host_exports = host_exports::test_support::HostExports::new(&ctx);

        let metrics_registry: Arc<MetricsRegistry> = Arc::new(MetricsRegistry::mock());
        let stopwatch = StopwatchMetrics::new(
            ctx.logger.clone(),
            deployment.hash.clone(),
            "test",
            metrics_registry.clone(),
            "test_shard".to_string(),
        );
        let gas_metrics = GasMetrics::new(deployment.hash.clone(), metrics_registry);

        let gas = GasCounter::new(gas_metrics);

        Host {
            ctx,
            host_exports,
            stopwatch,
            gas,
        }
    }

    fn store_set(
        &mut self,
        entity_type: &str,
        id: &str,
        data: Vec<(&str, &str)>,
    ) -> Result<(), HostExportError> {
        let data: Vec<_> = data.into_iter().map(|(k, v)| (k, Value::from(v))).collect();
        self.store_setv(entity_type, id, data)
    }

    fn store_setv(
        &mut self,
        entity_type: &str,
        id: &str,
        data: Vec<(&str, Value)>,
    ) -> Result<(), HostExportError> {
        let id = String::from(id);
        let data = HashMap::from_iter(data.into_iter().map(|(k, v)| (Word::from(k), v)));
        self.host_exports.store_set(
            &self.ctx.logger,
            &mut self.ctx.state,
            12, // Arbitrary block number
            &self.ctx.proof_of_indexing,
            entity_type.to_string(),
            id,
            data,
            &self.stopwatch,
            &self.gas,
        )
    }

    fn store_get(
        &mut self,
        entity_type: &str,
        id: &str,
    ) -> Result<Option<Cow<Entity>>, anyhow::Error> {
        let user_id = String::from(id);
        self.host_exports.store_get(
            &mut self.ctx.state,
            entity_type.to_string(),
            user_id,
            &self.gas,
        )
    }
}

/// Test the various ways in which `store_set` sets the `id` of entities and
/// errors when there are issues
#[tokio::test]
async fn test_store_set_id() {
    #[track_caller]
    fn err_says<E: std::fmt::Debug + std::fmt::Display>(err: E, exp: &str) {
        let err = err.to_string();
        assert!(err.contains(exp), "expected `{err}` to contain `{exp}`");
    }

    const UID: &str = "u1";
    const USER: &str = "User";
    const BID: &str = "0xdeadbeef";
    const BINARY: &str = "Binary";

    let schema = "type User @entity {
        id: ID!,
        name: String,
    }

    type Binary @entity {
        id: Bytes!,
        name: String,
    }";

    let mut host = Host::new(schema, "hostStoreSetId", "boolean.wasm", None).await;

    host.store_set(USER, UID, vec![("id", "u1"), ("name", "user1")])
        .expect("setting with same id works");

    let err = host
        .store_set(USER, UID, vec![("id", "ux"), ("name", "user1")])
        .expect_err("setting with different id fails");
    err_says(err, "conflicts with ID passed");

    host.store_set(USER, UID, vec![("name", "user2")])
        .expect("setting with no id works");

    let entity = host.store_get(USER, UID).unwrap().unwrap();
    assert_eq!(
        "u1",
        entity.id().to_string(),
        "store.set sets id automatically"
    );

    let beef = Value::Bytes("0xbeef".parse().unwrap());
    let err = host
        .store_setv(USER, "0xbeef", vec![("id", beef)])
        .expect_err("setting with Bytes id fails");
    err_says(
        err,
        "Attribute `User.id` has wrong type: expected String but got Bytes",
    );

    host.store_setv(USER, UID, vec![("id", Value::Int(32))])
        .expect_err("id must be a string");

    //
    // Now for bytes id
    //
    let bid_bytes = Value::Bytes(BID.parse().unwrap());

    let err = host
        .store_set(BINARY, BID, vec![("id", BID), ("name", "user1")])
        .expect_err("setting with string id in values fails");
    err_says(
        err,
        "Attribute `Binary.id` has wrong type: expected Bytes but got String",
    );

    host.store_setv(
        BINARY,
        BID,
        vec![("id", bid_bytes), ("name", Value::from("user1"))],
    )
    .expect("setting with bytes id in values works");

    let beef = Value::Bytes("0xbeef".parse().unwrap());
    let err = host
        .store_setv(BINARY, BID, vec![("id", beef)])
        .expect_err("setting with different id fails");
    err_says(err, "conflicts with ID passed");

    host.store_set(BINARY, BID, vec![("name", "user2")])
        .expect("setting with no id works");

    let entity = host.store_get(BINARY, BID).unwrap().unwrap();
    assert_eq!(
        BID,
        entity.id().to_string(),
        "store.set sets id automatically"
    );

    let err = host
        .store_setv(BINARY, BID, vec![("id", Value::Int(32))])
        .expect_err("id must be Bytes");
    err_says(
        err,
        "Attribute `Binary.id` has wrong type: expected Bytes but got Int",
    );
}

/// Test setting fields that are not defined in the schema
/// This should return an error
#[tokio::test]
async fn test_store_set_invalid_fields() {
    #[track_caller]
    fn err_says<E: std::fmt::Debug + std::fmt::Display>(err: E, exp: &str) {
        let err = err.to_string();
        assert!(err.contains(exp), "expected `{err}` to contain `{exp}`");
    }

    const UID: &str = "u1";
    const USER: &str = "User";
    const BID: &str = "0xdeadbeef";
    const BINARY: &str = "Binary";
    let schema = "
    type User @entity {
        id: ID!,
        name: String
    }

    type Binary @entity {
        id: Bytes!,
        test: String,
        test2: String
    }";

    let mut host = Host::new(
        schema,
        "hostStoreSetInvalidFields",
        "boolean.wasm",
        Some(API_VERSION_0_0_8),
    )
    .await;

    host.store_set(USER, UID, vec![("id", "u1"), ("name", "user1")])
        .unwrap();

    let err = host
        .store_set(
            USER,
            UID,
            vec![
                ("id", "u1"),
                ("name", "user1"),
                ("test", "invalid_field"),
                ("test2", "invalid_field"),
            ],
        )
        .err()
        .unwrap();

    // The order of `test` and `test2` is not guranteed
    // So we just check the string contains them
    let err_string = err.to_string();
    dbg!(err_string.as_str());
    assert!(err_string.contains("Attempted to set undefined fields [test, test2] for the entity type `User`. Make sure those fields are defined in the schema."));

    let err = host
        .store_set(
            USER,
            UID,
            vec![("id", "u1"), ("name", "user1"), ("test3", "invalid_field")],
        )
        .err()
        .unwrap();

    err_says(err, "Attempted to set undefined fields [test3] for the entity type `User`. Make sure those fields are defined in the schema.");

    // For apiVersion below 0.0.8, we should not error out
    let mut host2 = Host::new(
        schema,
        "hostStoreSetInvalidFields",
        "boolean.wasm",
        Some(API_VERSION_0_0_7),
    )
    .await;

    let err_is_none = host2
        .store_set(
            USER,
            UID,
            vec![
                ("id", "u1"),
                ("name", "user1"),
                ("test", "invalid_field"),
                ("test2", "invalid_field"),
            ],
        )
        .err()
        .is_none();

    assert!(err_is_none);
}

/// Test generating ids through `store_set`
#[tokio::test]
async fn generate_id() {
    const AUTO: &str = "auto";
    const INT8: &str = "Int8";
    const BINARY: &str = "Binary";

    let schema = "type Int8 @entity(immutable: true) {
        id: Int8!,
        name: String,
    }

    type Binary @entity(immutable: true) {
        id: Bytes!,
        name: String,
    }";

    let mut host = Host::new(schema, "hostGenerateId", "boolean.wasm", None).await;

    // Since these entities are immutable, storing twice would generate an
    // error; but since the ids are autogenerated, each invocation creates a
    // new id. Note that the types of the ids have an incorrect type, but
    // that doesn't matter since they get overwritten.
    host.store_set(INT8, AUTO, vec![("id", "u1"), ("name", "int1")])
        .expect("setting auto works");
    host.store_set(INT8, AUTO, vec![("id", "u1"), ("name", "int2")])
        .expect("setting auto works");
    host.store_set(BINARY, AUTO, vec![("id", "u1"), ("name", "bin1")])
        .expect("setting auto works");
    host.store_set(BINARY, AUTO, vec![("id", "u1"), ("name", "bin2")])
        .expect("setting auto works");

    let entity_cache = host.ctx.state.entity_cache;
    let mods = entity_cache.as_modifications(12).unwrap().modifications;
    let id_map: HashMap<&str, Id> = HashMap::from_iter(
        vec![
            (
                "bin1",
                IdType::Bytes.parse("0x0000000c00000002".into()).unwrap(),
            ),
            (
                "bin2",
                IdType::Bytes.parse("0x0000000c00000003".into()).unwrap(),
            ),
            ("int1", Id::Int8(0x0000_000c__0000_0000)),
            ("int2", Id::Int8(0x0000_000c__0000_0001)),
        ]
        .into_iter(),
    );
    assert_eq!(4, mods.len());
    for m in &mods {
        match m {
            EntityModification::Insert { data, .. } => {
                let id = data.get("id").unwrap();
                let name = data.get("name").unwrap().as_str().unwrap();
                let exp = id_map.get(name).unwrap();
                assert_eq!(exp, id, "Wrong id for entity with name `{name}`");
            }
            _ => panic!("expected Insert modification"),
        }
    }
}
