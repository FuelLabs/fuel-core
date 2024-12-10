use fuel_core_executor::executor::{
    ExecutionOptions,
    ExecutionResult,
};
use fuel_core_storage::transactional::Changes;
use fuel_core_types::{
    blockchain::block::Block,
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            ProductionResult,
            UncommittedResult,
            ValidationResult,
        },
        Uncommitted,
    },
};
#[cfg(feature = "std")]
use fuel_core_types_v0::services::executor::Error as ExecutorErrorV0;

/// Pack a pointer and length into an `u64`.
pub fn pack_ptr_and_len(ptr: u32, len: u32) -> u64 {
    (u64::from(len) << 32) | u64::from(ptr)
}

/// Unpacks an `u64` into the pointer and length.
pub fn unpack_ptr_and_len(val: u64) -> (u32, u32) {
    let ptr = u32::try_from(val & (u32::MAX as u64))
        .expect("It only contains first 32 bytes; qed");
    let len = u32::try_from(val >> 32).expect("It only contains first 32 bytes; qed");

    (ptr, len)
}

/// Pack a `exists`, `size` and `result` into one `u64`.
pub fn pack_exists_size_result(exists: bool, size: u32, result: u16) -> u64 {
    (u64::from(result) << 33 | u64::from(size) << 1) | u64::from(exists)
}

/// Unpacks an `u64` into `exists`, `size` and `result`.
pub fn unpack_exists_size_result(val: u64) -> (bool, u32, u16) {
    let exists = (val & 1u64) != 0;
    let size = u32::try_from((val >> 1) & (u32::MAX as u64))
        .expect("It only contains first 32 bytes; qed");
    let result = u16::try_from(val >> 33 & (u16::MAX as u64))
        .expect("It only contains first 16 bytes; qed");

    (exists, size, result)
}

/// The input type for the WASM executor. Enum allows handling different
/// versions of the input without introducing new host functions.
#[derive(Debug, serde::Serialize)]
pub enum InputSerializationType<'a> {
    V1 {
        block: WasmSerializationBlockTypes<'a, ()>,
        options: ExecutionOptions,
    },
}

#[derive(Debug, serde::Deserialize)]
pub enum InputDeserializationType {
    V1 {
        block: WasmDeserializationBlockTypes<()>,
        options: ExecutionOptions,
    },
}

#[derive(Debug, serde::Serialize)]
pub enum WasmSerializationBlockTypes<'a, TxSource> {
    /// DryRun mode where P is being produced.
    DryRun(Components<TxSource>),
    /// Production mode where P is being produced.
    Production(Components<TxSource>),
    /// Validation of a produced block.
    Validation(&'a Block),
}

#[derive(Debug, serde::Deserialize)]
pub enum WasmDeserializationBlockTypes<TxSource> {
    /// DryRun mode where P is being produced.
    DryRun(Components<TxSource>),
    /// Production mode where P is being produced.
    Production(Components<TxSource>),
    /// Validation of a produced block.
    Validation(Block),
}

/// The JSON version of the executor error. The serialization and deserialization
/// of the JSON error are less sensitive to the order of the variants in the enum.
/// It simplifies the error conversion between different versions of the execution.
///
/// If deserialization fails, it returns a string representation of the error that
/// still has useful information, even if the error is not supported by the native executor.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct JSONError(String);

#[cfg(feature = "std")]
impl From<ExecutorErrorV0> for JSONError {
    fn from(value: ExecutorErrorV0) -> Self {
        let json = serde_json::to_string(&value).unwrap_or_else(|e| {
            anyhow::anyhow!("Failed to serialize the V0 error: {:?}", e).to_string()
        });
        JSONError(json)
    }
}

impl From<ExecutorError> for JSONError {
    fn from(value: ExecutorError) -> Self {
        let json = serde_json::to_string(&value).unwrap_or_else(|e| {
            anyhow::anyhow!("Failed to serialize the error: {:?}", e).to_string()
        });
        JSONError(json)
    }
}

impl From<JSONError> for ExecutorError {
    fn from(value: JSONError) -> Self {
        serde_json::from_str(&value.0).unwrap_or(ExecutorError::Other(value.0))
    }
}

/// The return type for the WASM executor. Enum allows handling different
/// versions of the return without introducing new host functions.
#[cfg(feature = "std")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ReturnType {
    ProductionV0(
        Result<Uncommitted<ProductionResult<ExecutorErrorV0>, Changes>, ExecutorErrorV0>,
    ),
    ProductionV1(Result<Uncommitted<ProductionResult<JSONError>, Changes>, JSONError>),
    Validation(Result<Uncommitted<ValidationResult, Changes>, JSONError>),
    Execution(Result<ExecutionResult<JSONError>, JSONError>),
}

/// The return type for the WASM executor. Enum allows handling different
/// versions of the return without introducing new host functions.
#[cfg(not(feature = "std"))]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ReturnType {
    /// WASM executor doesn't use this variant, so from its perspective it is empty.
    ProductionV0,
    ProductionV1(Result<Uncommitted<ProductionResult<JSONError>, Changes>, JSONError>),
    Validation(Result<Uncommitted<ValidationResult, Changes>, JSONError>),
    Execution(Result<ExecutionResult<JSONError>, JSONError>),
}

/// Converts the latest execution result to the `ProductionV1`.
pub fn convert_to_v1_production_result(
    result: Result<UncommittedResult<Changes>, ExecutorError>,
) -> Result<Uncommitted<ProductionResult<JSONError>, Changes>, JSONError> {
    result
        .map(|result| {
            let (result, changes) = result.into();
            let ProductionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            } = result;

            let skipped_transactions: Vec<_> = skipped_transactions
                .into_iter()
                .map(|(id, error)| (id, JSONError::from(error)))
                .collect();

            let result = ProductionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            };

            Uncommitted::new(result, changes)
        })
        .map_err(JSONError::from)
}

/// Converts the `ProductionV1` to latest execution result.
pub fn convert_from_v1_production_result(
    result: Result<Uncommitted<ProductionResult<JSONError>, Changes>, JSONError>,
) -> Result<UncommittedResult<Changes>, ExecutorError> {
    result
        .map(|result| {
            let (result, changes) = result.into();
            let ProductionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            } = result;

            let skipped_transactions: Vec<_> = skipped_transactions
                .into_iter()
                .map(|(id, error)| (id, ExecutorError::from(error)))
                .collect();

            let result = ProductionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            };

            Uncommitted::new(result, changes)
        })
        .map_err(ExecutorError::from)
}

/// Converts the `ProductionV0` to latest execution result.
#[cfg(feature = "std")]
pub fn convert_from_v0_production_result(
    result: Result<
        Uncommitted<ProductionResult<ExecutorErrorV0>, Changes>,
        ExecutorErrorV0,
    >,
) -> Result<UncommittedResult<Changes>, ExecutorError> {
    result
        .map(|result| {
            let (result, changes) = result.into();
            let ProductionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            } = result;

            let skipped_transactions: Vec<_> = skipped_transactions
                .into_iter()
                .map(|(id, error)| (id, ExecutorError::from(JSONError::from(error))))
                .collect();

            let result = ProductionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            };

            Uncommitted::new(result, changes)
        })
        .map_err(JSONError::from)
        .map_err(ExecutorError::from)
}

/// Converts the transactions execution result to the `Execution`.
pub fn convert_to_execution_result(
    result: Result<ExecutionResult, ExecutorError>,
) -> Result<ExecutionResult<JSONError>, JSONError> {
    result
        .map(|result| {
            let ExecutionResult {
                partial_block,
                mut execution_data,
            } = result;
            let skipped_transactions =
                core::mem::take(&mut execution_data.skipped_transactions);

            let skipped_transactions: Vec<_> = skipped_transactions
                .into_iter()
                .map(|(id, error)| (id, JSONError::from(error)))
                .collect();

            let execution_data =
                execution_data.with_skipped_transactions(skipped_transactions);

            ExecutionResult {
                partial_block,
                execution_data,
            }
        })
        .map_err(JSONError::from)
}

/// Converts the `Execution` to `ExecutionResult`.
#[cfg(feature = "std")]
pub fn convert_from_execution_result(
    result: Result<ExecutionResult<JSONError>, JSONError>,
) -> Result<ExecutionResult, ExecutorError> {
    result
        .map(|result| {
            let ExecutionResult {
                partial_block,
                mut execution_data,
            } = result;

            let skipped_transactions =
                core::mem::take(&mut execution_data.skipped_transactions);

            let skipped_transactions: Vec<_> = skipped_transactions
                .into_iter()
                .map(|(id, error)| (id, ExecutorError::from(error)))
                .collect();

            let execution_data =
                execution_data.with_skipped_transactions(skipped_transactions);

            ExecutionResult {
                partial_block,
                execution_data,
            }
        })
        .map_err(ExecutorError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::services::executor::TransactionValidityError;
    #[cfg(feature = "std")]
    use fuel_core_types_v0::services::executor::TransactionValidityError as TransactionValidityErrorV0;
    use proptest::prelude::prop::*;

    proptest::proptest! {
        #[test]
        fn can_pack_any_values(exists: bool, size: u32, result: u16) {
            pack_exists_size_result(exists, size, result);
        }

        #[test]
        fn can_unpack_any_values(value: u64) {
            let _ = unpack_exists_size_result(value);
        }


        #[test]
        fn unpacks_packed_values(exists: bool, size: u32, result: u16) {
            let packed = pack_exists_size_result(exists, size, result);
            let (unpacked_exists, unpacked_size, unpacked_result) =
                unpack_exists_size_result(packed);

            proptest::prop_assert_eq!(exists, unpacked_exists);
            proptest::prop_assert_eq!(size, unpacked_size);
            proptest::prop_assert_eq!(result, unpacked_result);
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn can_convert_v0_error_to_v1() {
        // Given
        let v0 = ExecutorErrorV0::TransactionValidity(
            TransactionValidityErrorV0::CoinDoesNotExist(Default::default()),
        );

        // When
        let json: JSONError = v0.into();
        let v1: ExecutorError = json.into();

        // Then
        assert_eq!(
            v1,
            ExecutorError::TransactionValidity(
                TransactionValidityError::CoinDoesNotExist(Default::default())
            )
        );
    }
}
