use crate::protobuf;
use graph::prelude::tokio;

use self::data::BadFixed;

const WASM_FILE_NAME: &str = "test_padding.wasm";

//for tests, to run in parallel, sub graph name has be unique
fn rnd_sub_graph_name(size: usize) -> String {
    use rand::{distributions::Alphanumeric, Rng};
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(size)
        .map(char::from)
        .collect()
}

pub mod data {
    #[graph_runtime_derive::generate_asc_type()]
    #[graph_runtime_derive::generate_network_type_id(UnitTestNetwork)]
    #[graph_runtime_derive::generate_from_rust_type()]
    #[graph_runtime_derive::generate_array_type(UnitTestNetwork)]
    #[derive(Debug, PartialEq)]
    pub struct UnitTestTypeBool {
        pub str_pref: String,
        pub under_test: bool,
        pub str_suff: String,
        pub large: i64,
        pub tail: bool,
    }

    #[graph_runtime_derive::generate_asc_type()]
    #[graph_runtime_derive::generate_network_type_id(UnitTestNetwork)]
    #[graph_runtime_derive::generate_from_rust_type()]
    #[graph_runtime_derive::generate_array_type(UnitTestNetwork)]
    #[derive(Debug, PartialEq)]
    pub struct UnitTestTypeI8 {
        pub str_pref: String,
        pub under_test: i8,
        pub str_suff: String,
        pub large: i64,
        pub tail: bool,
    }
    #[graph_runtime_derive::generate_asc_type()]
    #[graph_runtime_derive::generate_network_type_id(UnitTestNetwork)]
    #[graph_runtime_derive::generate_from_rust_type()]
    #[graph_runtime_derive::generate_array_type(UnitTestNetwork)]
    #[derive(Debug, PartialEq)]
    pub struct UnitTestTypeU16 {
        pub str_pref: String,
        pub under_test: u16,
        pub str_suff: String,
        pub large: i64,
        pub tail: bool,
    }
    #[graph_runtime_derive::generate_asc_type()]
    #[graph_runtime_derive::generate_network_type_id(UnitTestNetwork)]
    #[graph_runtime_derive::generate_from_rust_type()]
    #[graph_runtime_derive::generate_array_type(UnitTestNetwork)]
    #[derive(Debug, PartialEq)]
    pub struct UnitTestTypeU32 {
        pub str_pref: String,
        pub under_test: u32,
        pub str_suff: String,
        pub large: i64,
        pub tail: bool,
    }

    pub struct Bad {
        pub nonce: u64,
        pub str_suff: String,
        pub tail: u64,
    }

    #[repr(C)]
    pub struct AscBad {
        pub nonce: u64,
        pub str_suff: graph::runtime::AscPtr<graph_runtime_wasm::asc_abi::class::AscString>,
        pub tail: u64,
    }

    impl AscType for AscBad {
        fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
            let in_memory_byte_count = std::mem::size_of::<Self>();
            let mut bytes = Vec::with_capacity(in_memory_byte_count);

            bytes.extend_from_slice(&self.nonce.to_asc_bytes()?);
            bytes.extend_from_slice(&self.str_suff.to_asc_bytes()?);
            bytes.extend_from_slice(&self.tail.to_asc_bytes()?);

            //ensure misaligned
            assert!(
                bytes.len() != in_memory_byte_count,
                "struct is intentionally misaligned",
            );
            Ok(bytes)
        }

        fn from_asc_bytes(
            _asc_obj: &[u8],
            _api_version: &graph::semver::Version,
        ) -> Result<Self, DeterministicHostError> {
            unimplemented!();
        }
    }

    impl AscIndexId for AscBad {
        const INDEX_ASC_TYPE_ID: IndexForAscTypeId =
            IndexForAscTypeId::UnitTestNetworkUnitTestTypeBool;
    }

    use graph::runtime::HostExportError;
    pub use graph::runtime::{
        asc_new, gas::GasCounter, AscHeap, AscIndexId, AscPtr, AscType, AscValue,
        DeterministicHostError, IndexForAscTypeId, ToAscObj,
    };

    impl ToAscObj<AscBad> for Bad {
        fn to_asc_obj<H: AscHeap + ?Sized>(
            &self,
            heap: &mut H,
            gas: &GasCounter,
        ) -> Result<AscBad, HostExportError> {
            Ok(AscBad {
                nonce: self.nonce,
                str_suff: asc_new(heap, &self.str_suff, gas)?,
                tail: self.tail,
            })
        }
    }

    pub struct BadFixed {
        pub nonce: u64,
        pub str_suff: String,
        pub tail: u64,
    }
    #[repr(C)]
    pub struct AscBadFixed {
        pub nonce: u64,
        pub str_suff: graph::runtime::AscPtr<graph_runtime_wasm::asc_abi::class::AscString>,
        pub _padding: u32,
        pub tail: u64,
    }

    impl AscType for AscBadFixed {
        fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
            let in_memory_byte_count = std::mem::size_of::<Self>();
            let mut bytes = Vec::with_capacity(in_memory_byte_count);

            bytes.extend_from_slice(&self.nonce.to_asc_bytes()?);
            bytes.extend_from_slice(&self.str_suff.to_asc_bytes()?);
            bytes.extend_from_slice(&self._padding.to_asc_bytes()?);
            bytes.extend_from_slice(&self.tail.to_asc_bytes()?);

            assert_eq!(
                bytes.len(),
                in_memory_byte_count,
                "Alignment mismatch for AscBadFixed, re-order fields or explicitely add a _padding field",
            );
            Ok(bytes)
        }

        fn from_asc_bytes(
            _asc_obj: &[u8],
            _api_version: &graph::semver::Version,
        ) -> Result<Self, DeterministicHostError> {
            unimplemented!();
        }
    }

    //we will have to keep this chain specific (Inner/Outer)
    impl AscIndexId for AscBadFixed {
        const INDEX_ASC_TYPE_ID: IndexForAscTypeId =
            IndexForAscTypeId::UnitTestNetworkUnitTestTypeBool;
    }

    impl ToAscObj<AscBadFixed> for BadFixed {
        fn to_asc_obj<H: AscHeap + ?Sized>(
            &self,
            heap: &mut H,
            gas: &GasCounter,
        ) -> Result<AscBadFixed, HostExportError> {
            Ok(AscBadFixed {
                nonce: self.nonce,
                str_suff: asc_new(heap, &self.str_suff, gas)?,
                _padding: 0,
                tail: self.tail,
            })
        }
    }
}

#[tokio::test]
async fn test_v5_manual_padding_manualy_fixed_ok() {
    manual_padding_manualy_fixed_ok(super::test::API_VERSION_0_0_5).await
}

#[tokio::test]
async fn test_v4_manual_padding_manualy_fixed_ok() {
    manual_padding_manualy_fixed_ok(super::test::API_VERSION_0_0_4).await
}

#[tokio::test]
async fn test_v5_manual_padding_should_fail() {
    manual_padding_should_fail(super::test::API_VERSION_0_0_5).await
}

#[tokio::test]
async fn test_v4_manual_padding_should_fail() {
    manual_padding_should_fail(super::test::API_VERSION_0_0_4).await
}

#[tokio::test]
async fn test_v5_bool_padding_ok() {
    bool_padding_ok(super::test::API_VERSION_0_0_5).await
}

#[tokio::test]
async fn test_v4_bool_padding_ok() {
    bool_padding_ok(super::test::API_VERSION_0_0_4).await
}

#[tokio::test]
async fn test_v5_i8_padding_ok() {
    i8_padding_ok(super::test::API_VERSION_0_0_5).await
}

#[tokio::test]
async fn test_v4_i8_padding_ok() {
    i8_padding_ok(super::test::API_VERSION_0_0_4).await
}

#[tokio::test]
async fn test_v5_u16_padding_ok() {
    u16_padding_ok(super::test::API_VERSION_0_0_5).await
}

#[tokio::test]
async fn test_v4_u16_padding_ok() {
    u16_padding_ok(super::test::API_VERSION_0_0_4).await
}

#[tokio::test]
async fn test_v5_u32_padding_ok() {
    u32_padding_ok(super::test::API_VERSION_0_0_5).await
}

#[tokio::test]
async fn test_v4_u32_padding_ok() {
    u32_padding_ok(super::test::API_VERSION_0_0_4).await
}

async fn manual_padding_should_fail(api_version: semver::Version) {
    let mut module = super::test::test_module(
        &rnd_sub_graph_name(12),
        super::common::mock_data_source(
            &super::test::wasm_file_path(WASM_FILE_NAME, api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let parm = protobuf::Bad {
        nonce: i64::MAX as u64,
        str_suff: "suff".into(),
        tail: i64::MAX as u64,
    };

    let new_obj = module.asc_new(&parm).unwrap();

    let func = module
        .get_func("test_padding_manual")
        .typed()
        .unwrap()
        .clone();

    let res: Result<(), _> = func.call(new_obj.wasm_ptr());

    assert!(
        res.is_err(),
        "suposed to fail due to WASM memory padding error"
    );
}

async fn manual_padding_manualy_fixed_ok(api_version: semver::Version) {
    let parm = BadFixed {
        nonce: i64::MAX as u64,
        str_suff: "suff".into(),
        tail: i64::MAX as u64,
    };

    let mut module = super::test::test_module(
        &rnd_sub_graph_name(12),
        super::common::mock_data_source(
            &super::test::wasm_file_path(WASM_FILE_NAME, api_version.clone()),
            api_version.clone(),
        ),
        api_version.clone(),
    )
    .await;

    let new_obj = module.asc_new(&parm).unwrap();

    let func = module
        .get_func("test_padding_manual")
        .typed()
        .unwrap()
        .clone();

    let res: Result<(), _> = func.call(new_obj.wasm_ptr());

    assert!(res.is_ok(), "{:?}", res.err());
}

async fn bool_padding_ok(api_version: semver::Version) {
    let mut module = super::test::test_module(
        &rnd_sub_graph_name(12),
        super::common::mock_data_source(
            &super::test::wasm_file_path(WASM_FILE_NAME, api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let parm = protobuf::UnitTestTypeBool {
        str_pref: "pref".into(),
        under_test: true,
        str_suff: "suff".into(),
        large: i64::MAX,
        tail: true,
    };

    let new_obj = module.asc_new(&parm).unwrap();

    let func = module
        .get_func("test_padding_bool")
        .typed()
        .unwrap()
        .clone();

    let res: Result<(), _> = func.call(new_obj.wasm_ptr());

    assert!(res.is_ok(), "{:?}", res.err());
}

async fn i8_padding_ok(api_version: semver::Version) {
    let mut module = super::test::test_module(
        &rnd_sub_graph_name(12),
        super::common::mock_data_source(
            &super::test::wasm_file_path(WASM_FILE_NAME, api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let parm = protobuf::UnitTestTypeI8 {
        str_pref: "pref".into(),
        under_test: i8::MAX,
        str_suff: "suff".into(),
        large: i64::MAX,
        tail: true,
    };

    let new_obj = module.asc_new(&parm).unwrap();

    let func = module.get_func("test_padding_i8").typed().unwrap().clone();

    let res: Result<(), _> = func.call(new_obj.wasm_ptr());

    assert!(res.is_ok(), "{:?}", res.err());
}

async fn u16_padding_ok(api_version: semver::Version) {
    let mut module = super::test::test_module(
        &rnd_sub_graph_name(12),
        super::common::mock_data_source(
            &super::test::wasm_file_path(WASM_FILE_NAME, api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let parm = protobuf::UnitTestTypeU16 {
        str_pref: "pref".into(),
        under_test: i16::MAX as u16,
        str_suff: "suff".into(),
        large: i64::MAX,
        tail: true,
    };

    let new_obj = module.asc_new(&parm).unwrap();

    let func = module.get_func("test_padding_i16").typed().unwrap().clone();

    let res: Result<(), _> = func.call(new_obj.wasm_ptr());

    assert!(res.is_ok(), "{:?}", res.err());
}

async fn u32_padding_ok(api_version: semver::Version) {
    let mut module = super::test::test_module(
        &rnd_sub_graph_name(12),
        super::common::mock_data_source(
            &super::test::wasm_file_path(WASM_FILE_NAME, api_version.clone()),
            api_version.clone(),
        ),
        api_version,
    )
    .await;

    let parm = protobuf::UnitTestTypeU32 {
        str_pref: "pref".into(),
        under_test: i32::MAX as u32,
        str_suff: "suff".into(),
        large: i64::MAX,
        tail: true,
    };

    let new_obj = module.asc_new(&parm).unwrap();

    let func = module.get_func("test_padding_i32").typed().unwrap().clone();

    let res: Result<(), _> = func.call(new_obj.wasm_ptr());

    assert!(res.is_ok(), "{:?}", res.err());
}
