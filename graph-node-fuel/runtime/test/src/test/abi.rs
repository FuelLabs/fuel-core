use graph::prelude::{ethabi::Token, web3::types::U256};
use graph_runtime_wasm::asc_abi::class::{
    ArrayBuffer, AscAddress, AscEnum, AscEnumArray, EthereumValueKind, StoreValueKind, TypedArray,
};

use super::*;

async fn test_unbounded_loop(api_version: Version) {
    // Set handler timeout to 3 seconds.
    let module = test_valid_module_and_store_with_timeout(
        "unboundedLoop",
        mock_data_source(
            &wasm_file_path("non_terminating.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
        Some(Duration::from_secs(3)),
    )
    .await
    .0;
    let res: Result<(), _> = module.get_func("loop").typed().unwrap().call(());
    assert_eq!(
        res.unwrap_err().to_string().lines().next().unwrap(),
        "wasm trap: interrupt"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unbounded_loop_v0_0_4() {
    test_unbounded_loop(API_VERSION_0_0_4).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn unbounded_loop_v0_0_5() {
    test_unbounded_loop(API_VERSION_0_0_5).await;
}

async fn test_unbounded_recursion(api_version: Version) {
    let module = test_module(
        "unboundedRecursion",
        mock_data_source(
            &wasm_file_path("non_terminating.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;
    let res: Result<(), _> = module.get_func("rabbit_hole").typed().unwrap().call(());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("call stack exhausted"), "{:#?}", err_msg);
}

#[tokio::test]
async fn unbounded_recursion_v0_0_4() {
    test_unbounded_recursion(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn unbounded_recursion_v0_0_5() {
    test_unbounded_recursion(API_VERSION_0_0_5).await;
}

async fn test_abi_array(api_version: Version, gas_used: u64) {
    let mut module = test_module(
        "abiArray",
        mock_data_source(
            &wasm_file_path("abi_classes.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let vec = vec![
        "1".to_owned(),
        "2".to_owned(),
        "3".to_owned(),
        "4".to_owned(),
    ];
    let new_vec_obj: AscPtr<Array<AscPtr<AscString>>> = module.invoke_export1("test_array", &vec);
    let new_vec: Vec<String> = module.asc_get(new_vec_obj).unwrap();

    assert_eq!(module.gas_used(), gas_used);
    assert_eq!(
        new_vec,
        vec![
            "1".to_owned(),
            "2".to_owned(),
            "3".to_owned(),
            "4".to_owned(),
            "5".to_owned(),
        ]
    );
}

#[tokio::test]
async fn abi_array_v0_0_4() {
    test_abi_array(API_VERSION_0_0_4, 695935).await;
}

#[tokio::test]
async fn abi_array_v0_0_5() {
    test_abi_array(API_VERSION_0_0_5, 1636130).await;
}

async fn test_abi_subarray(api_version: Version) {
    let mut module = test_module(
        "abiSubarray",
        mock_data_source(
            &wasm_file_path("abi_classes.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let vec: Vec<u8> = vec![1, 2, 3, 4];
    let new_vec_obj: AscPtr<TypedArray<u8>> =
        module.invoke_export1("byte_array_third_quarter", vec.as_slice());
    let new_vec: Vec<u8> = module.asc_get(new_vec_obj).unwrap();

    assert_eq!(new_vec, vec![3]);
}

#[tokio::test]
async fn abi_subarray_v0_0_4() {
    test_abi_subarray(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn abi_subarray_v0_0_5() {
    test_abi_subarray(API_VERSION_0_0_5).await;
}

async fn test_abi_bytes_and_fixed_bytes(api_version: Version) {
    let mut module = test_module(
        "abiBytesAndFixedBytes",
        mock_data_source(
            &wasm_file_path("abi_classes.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;
    let bytes1: Vec<u8> = vec![42, 45, 7, 245, 45];
    let bytes2: Vec<u8> = vec![3, 12, 0, 1, 255];
    let new_vec_obj: AscPtr<Uint8Array> = module.invoke_export2("concat", &*bytes1, &*bytes2);

    // This should be bytes1 and bytes2 concatenated.
    let new_vec: Vec<u8> = module.asc_get(new_vec_obj).unwrap();

    let mut concated = bytes1.clone();
    concated.extend(bytes2.clone());
    assert_eq!(new_vec, concated);
}

#[tokio::test]
async fn abi_bytes_and_fixed_bytes_v0_0_4() {
    test_abi_bytes_and_fixed_bytes(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn abi_bytes_and_fixed_bytes_v0_0_5() {
    test_abi_bytes_and_fixed_bytes(API_VERSION_0_0_5).await;
}

async fn test_abi_ethabi_token_identity(api_version: Version) {
    let mut module = test_module(
        "abiEthabiTokenIdentity",
        mock_data_source(
            &wasm_file_path("abi_token.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    // Token::Address
    let address = H160([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token_address = Token::Address(address);

    let new_address_obj: AscPtr<AscAddress> =
        module.invoke_export1("token_to_address", &token_address);

    let new_token_ptr = module.takes_ptr_returns_ptr("token_from_address", new_address_obj);
    let new_token = module.asc_get(new_token_ptr).unwrap();

    assert_eq!(token_address, new_token);

    // Token::Bytes
    let token_bytes = Token::Bytes(vec![42, 45, 7, 245, 45]);
    let new_bytes_obj: AscPtr<ArrayBuffer> = module.invoke_export1("token_to_bytes", &token_bytes);
    let new_token_ptr = module.takes_ptr_returns_ptr("token_from_bytes", new_bytes_obj);
    let new_token = module.asc_get(new_token_ptr).unwrap();

    assert_eq!(token_bytes, new_token);

    // Token::Int
    let int_token = Token::Int(U256([256, 453452345, 0, 42]));
    let new_int_obj: AscPtr<ArrayBuffer> = module.invoke_export1("token_to_int", &int_token);

    let new_token_ptr = module.takes_ptr_returns_ptr("token_from_int", new_int_obj);
    let new_token = module.asc_get(new_token_ptr).unwrap();

    assert_eq!(int_token, new_token);

    // Token::Uint
    let uint_token = Token::Uint(U256([256, 453452345, 0, 42]));

    let new_uint_obj: AscPtr<ArrayBuffer> = module.invoke_export1("token_to_uint", &uint_token);
    let new_token_ptr = module.takes_ptr_returns_ptr("token_from_uint", new_uint_obj);
    let new_token = module.asc_get(new_token_ptr).unwrap();

    assert_eq!(uint_token, new_token);
    assert_ne!(uint_token, int_token);

    // Token::Bool
    let token_bool = Token::Bool(true);

    let token_bool_ptr = module.asc_new(&token_bool).unwrap();
    let func = module.get_func("token_to_bool").typed().unwrap().clone();
    let boolean: i32 = func.call(token_bool_ptr.wasm_ptr()).unwrap();

    let new_token_ptr = module.takes_val_returns_ptr("token_from_bool", boolean);
    let new_token = module.asc_get(new_token_ptr).unwrap();

    assert_eq!(token_bool, new_token);

    // Token::String
    let token_string = Token::String("漢字Go🇧🇷".into());
    let new_string_obj: AscPtr<AscString> = module.invoke_export1("token_to_string", &token_string);
    let new_token_ptr = module.takes_ptr_returns_ptr("token_from_string", new_string_obj);
    let new_token = module.asc_get(new_token_ptr).unwrap();

    assert_eq!(token_string, new_token);

    // Token::Array
    let token_array = Token::Array(vec![token_address, token_bytes, token_bool]);
    let token_array_nested = Token::Array(vec![token_string, token_array]);
    let new_array_obj: AscEnumArray<EthereumValueKind> =
        module.invoke_export1("token_to_array", &token_array_nested);

    let new_token_ptr = module.takes_ptr_returns_ptr("token_from_array", new_array_obj);
    let new_token: Token = module.asc_get(new_token_ptr).unwrap();

    assert_eq!(new_token, token_array_nested);
}

/// Test a roundtrip Token -> Payload -> Token identity conversion through asc,
/// and assert the final token is the same as the starting one.
#[tokio::test]
async fn abi_ethabi_token_identity_v0_0_4() {
    test_abi_ethabi_token_identity(API_VERSION_0_0_4).await;
}

/// Test a roundtrip Token -> Payload -> Token identity conversion through asc,
/// and assert the final token is the same as the starting one.
#[tokio::test]
async fn abi_ethabi_token_identity_v0_0_5() {
    test_abi_ethabi_token_identity(API_VERSION_0_0_5).await;
}

async fn test_abi_store_value(api_version: Version) {
    let mut module = test_module(
        "abiStoreValue",
        mock_data_source(
            &wasm_file_path("abi_store_value.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    // Value::Null
    let func = module.get_func("value_null").typed().unwrap().clone();
    let ptr: u32 = func.call(()).unwrap();
    let null_value_ptr: AscPtr<AscEnum<StoreValueKind>> = ptr.into();
    let null_value: Value = module.asc_get(null_value_ptr).unwrap();
    assert_eq!(null_value, Value::Null);

    // Value::String
    let string = "some string";
    let new_value_ptr = module.invoke_export1("value_from_string", string);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(new_value, Value::from(string));

    // Value::Int
    let int = i32::min_value();
    let new_value_ptr = module.takes_val_returns_ptr("value_from_int", int);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(new_value, Value::Int(int));

    // Value::Int8
    let int8 = i64::min_value();
    let new_value_ptr = module.takes_val_returns_ptr("value_from_int8", int8);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(new_value, Value::Int8(int8));

    // Value::BigDecimal
    let big_decimal = BigDecimal::from_str("3.14159001").unwrap();
    let new_value_ptr = module.invoke_export1("value_from_big_decimal", &big_decimal);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(new_value, Value::BigDecimal(big_decimal));

    let big_decimal = BigDecimal::new(10.into(), 5);
    let new_value_ptr = module.invoke_export1("value_from_big_decimal", &big_decimal);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(new_value, Value::BigDecimal(1_000_000.into()));

    // Value::Bool
    let boolean = true;
    let new_value_ptr = module.takes_val_returns_ptr("value_from_bool", boolean as i32);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(new_value, Value::Bool(boolean));

    // Value::List
    let func = module
        .get_func("array_from_values")
        .typed()
        .unwrap()
        .clone();
    let new_value_ptr: u32 = func
        .call((module.asc_new(string).unwrap().wasm_ptr(), int))
        .unwrap();
    let new_value_ptr = AscPtr::from(new_value_ptr);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(
        new_value,
        Value::List(vec![Value::from(string), Value::Int(int)])
    );

    let array: &[Value] = &[
        Value::String("foo".to_owned()),
        Value::String("bar".to_owned()),
    ];
    let new_value_ptr = module.invoke_export1("value_from_array", array);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(
        new_value,
        Value::List(vec![
            Value::String("foo".to_owned()),
            Value::String("bar".to_owned()),
        ])
    );

    // Value::Bytes
    let bytes: &[u8] = &[0, 2, 5];
    let new_value_ptr = module.invoke_export1("value_from_bytes", bytes);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(new_value, Value::Bytes(bytes.into()));

    // Value::BigInt
    let bytes: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    let new_value_ptr = module.invoke_export1("value_from_bigint", bytes);
    let new_value: Value = module.asc_get(new_value_ptr).unwrap();
    assert_eq!(
        new_value,
        Value::BigInt(::graph::data::store::scalar::BigInt::from_unsigned_bytes_le(bytes).unwrap())
    );
}

#[tokio::test]
async fn abi_store_value_v0_0_4() {
    test_abi_store_value(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn abi_store_value_v0_0_5() {
    test_abi_store_value(API_VERSION_0_0_5).await;
}

async fn test_abi_h160(api_version: Version) {
    let mut module = test_module(
        "abiH160",
        mock_data_source(
            &wasm_file_path("abi_classes.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;
    let address = H160::zero();

    // As an `Uint8Array`
    let new_address_obj: AscPtr<Uint8Array> = module.invoke_export1("test_address", &address);

    // This should have 1 added to the first and last byte.
    let new_address: H160 = module.asc_get(new_address_obj).unwrap();

    assert_eq!(
        new_address,
        H160([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])
    )
}

#[tokio::test]
async fn abi_h160_v0_0_4() {
    test_abi_h160(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn abi_h160_v0_0_5() {
    test_abi_h160(API_VERSION_0_0_5).await;
}

async fn test_string(api_version: Version) {
    let mut module = test_module(
        "string",
        mock_data_source(
            &wasm_file_path("abi_classes.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;
    let string = "    漢字Double_Me🇧🇷  ";
    let trimmed_string_obj: AscPtr<AscString> = module.invoke_export1("repeat_twice", string);
    let doubled_string: String = module.asc_get(trimmed_string_obj).unwrap();
    assert_eq!(doubled_string, string.repeat(2));
}

#[tokio::test]
async fn string_v0_0_4() {
    test_string(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn string_v0_0_5() {
    test_string(API_VERSION_0_0_5).await;
}

async fn test_abi_big_int(api_version: Version) {
    let mut module = test_module(
        "abiBigInt",
        mock_data_source(
            &wasm_file_path("abi_classes.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    // Test passing in 0 and increment it by 1
    let old_uint = U256::zero();
    let new_uint_obj: AscPtr<AscBigInt> =
        module.invoke_export1("test_uint", &BigInt::from_unsigned_u256(&old_uint));
    let new_uint: BigInt = module.asc_get(new_uint_obj).unwrap();
    assert_eq!(new_uint, BigInt::from(1_i32));
    let new_uint = new_uint.to_unsigned_u256();
    assert_eq!(new_uint, U256([1, 0, 0, 0]));

    // Test passing in -50 and increment it by 1
    let old_uint = BigInt::from(-50);
    let new_uint_obj: AscPtr<AscBigInt> = module.invoke_export1("test_uint", &old_uint);
    let new_uint: BigInt = module.asc_get(new_uint_obj).unwrap();
    assert_eq!(new_uint, BigInt::from(-49_i32));
    let new_uint_from_u256 = BigInt::from_signed_u256(&new_uint.to_signed_u256());
    assert_eq!(new_uint, new_uint_from_u256);
}

#[tokio::test]
async fn abi_big_int_v0_0_4() {
    test_abi_big_int(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn abi_big_int_v0_0_5() {
    test_abi_big_int(API_VERSION_0_0_5).await;
}

async fn test_big_int_to_string(api_version: Version) {
    let mut module = test_module(
        "bigIntToString",
        mock_data_source(
            &wasm_file_path("big_int_to_string.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let big_int_str = "30145144166666665000000000000000000";
    let big_int = BigInt::from_str(big_int_str).unwrap();
    let string_obj: AscPtr<AscString> = module.invoke_export1("big_int_to_string", &big_int);
    let string: String = module.asc_get(string_obj).unwrap();
    assert_eq!(string, big_int_str);
}

#[tokio::test]
async fn big_int_to_string_v0_0_4() {
    test_big_int_to_string(API_VERSION_0_0_4).await;
}

#[tokio::test]
async fn big_int_to_string_v0_0_5() {
    test_big_int_to_string(API_VERSION_0_0_5).await;
}

async fn test_invalid_discriminant(api_version: Version) {
    let module = test_module(
        "invalidDiscriminant",
        mock_data_source(
            &wasm_file_path("abi_store_value.wasm", api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let func = module
        .get_func("invalid_discriminant")
        .typed()
        .unwrap()
        .clone();
    let ptr: u32 = func.call(()).unwrap();
    let _value: Value = module.asc_get(ptr.into()).unwrap();
}

// This should panic rather than exhibiting UB. It's hard to test for UB, but
// when reproducing a SIGILL was observed which would be caught by this.
#[tokio::test]
#[should_panic]
async fn invalid_discriminant_v0_0_4() {
    test_invalid_discriminant(API_VERSION_0_0_4).await;
}

// This should panic rather than exhibiting UB. It's hard to test for UB, but
// when reproducing a SIGILL was observed which would be caught by this.
#[tokio::test]
#[should_panic]
async fn invalid_discriminant_v0_0_5() {
    test_invalid_discriminant(API_VERSION_0_0_5).await;
}
