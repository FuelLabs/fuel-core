use crate::utils::{
    unpack_exists_size_result,
    InputDeserializationType,
};
use core::marker::PhantomData;
use fuel_core_executor::ports::MaybeCheckedTransaction;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::Transaction,
    services::relayer::Event,
};

/// A wrapper around a `u32` representing a pointer for WASM.
#[derive(Debug)]
#[repr(transparent)]
pub struct Ptr32<'a, T>
where
    T: ?Sized,
{
    /// The internal WASM raw pointer value.
    _value: u32,
    marker: PhantomData<fn() -> &'a T>,
}

impl<'a, T> Ptr32<'a, T>
where
    T: ?Sized,
{
    fn new(value: u32) -> Self {
        Self {
            _value: value,
            marker: Default::default(),
        }
    }
}

impl<'a, T> Ptr32<'a, [T]> {
    /// Creates a new WASM pointer from the given slice.
    pub fn from_slice(slice: &'a [T]) -> Self {
        Self::new(slice.as_ptr() as u32)
    }
}

/// A wrapper around a `u32` representing a pointer for WASM.
#[derive(Debug)]
#[repr(transparent)]
pub struct Ptr32Mut<'a, T>
where
    T: ?Sized,
{
    /// The internal WASM raw pointer value.
    _value: u32,
    marker: PhantomData<fn() -> &'a mut T>,
}

impl<'a, T> Ptr32Mut<'a, T>
where
    T: ?Sized,
{
    fn new(value: u32) -> Self {
        Self {
            _value: value,
            marker: Default::default(),
        }
    }
}

impl<'a, T> Ptr32Mut<'a, [T]> {
    /// Creates a new WASM pointer from the given slice.
    pub fn from_slice(slice: &'a mut [T]) -> Self {
        Self::new(slice.as_ptr() as u32)
    }
}

mod host {
    use super::*;

    #[link(wasm_import_module = "host_v0")]
    extern "C" {
        // Initialization API

        /// Returns the encoded input for the executor.
        pub(crate) fn input(output_ptr: Ptr32Mut<[u8]>, output_size: u32);

        // TxSource API

        /// Returns the size of the next encoded transactions.
        /// If the size is 0, there are no more transactions.
        pub(crate) fn peek_next_txs_size(gas_limit: u64) -> u32;

        /// Consumes the next transactions from the host.
        /// Calling this function before `peek_next_txs_size` do nothing.
        pub(crate) fn consume_next_txs(output_ptr: Ptr32Mut<[u8]>, output_size: u32);

        // Storage API

        /// Returns the size of the value from the storage.
        pub(crate) fn storage_size_of_value(
            key_ptr: Ptr32<[u8]>,
            key_len: u32,
            column: u32,
        ) -> u64;

        /// Returns the value from the storage.
        pub(crate) fn storage_get(
            key_ptr: Ptr32<[u8]>,
            key_len: u32,
            column: u32,
            out_ptr: Ptr32Mut<[u8]>,
            out_len: u32,
        ) -> ReturnResult;

        // Relayer API

        /// Returns the `true` of relayer is enabled.
        pub(crate) fn relayer_enabled() -> bool;

        /// Returns the size of the encoded events at `da_block_height`.
        pub(crate) fn relayer_size_of_events(da_block_height: u64) -> u64;

        /// Writes the encoded events at `da_block_height` into the `output_ptr`.
        pub(crate) fn relayer_get_events(
            da_block_height: u64,
            output_ptr: Ptr32Mut<[u8]>,
        );
    }
}

/// The result returned by the host function.
#[repr(transparent)]
pub struct ReturnResult(u16);

/// Gets the `InputType` by using the host function. The `size` is the size of the encoded input.
pub fn input(size: usize) -> anyhow::Result<InputDeserializationType> {
    let mut encoded_block = vec![0u8; size];
    let size = encoded_block.len();
    unsafe {
        host::input(
            Ptr32Mut::from_slice(encoded_block.as_mut_slice()),
            u32::try_from(size).expect("We only support wasm32 target; qed"),
        )
    };

    let input: InputDeserializationType = postcard::from_bytes(&encoded_block)
        .map_err(|e| anyhow::anyhow!("Failed to decode the block: {:?}", e))?;

    Ok(input)
}

/// Gets the next transactions by using the host function.
pub fn next_transactions(gas_limit: u64) -> anyhow::Result<Vec<MaybeCheckedTransaction>> {
    let next_size = unsafe { host::peek_next_txs_size(gas_limit) };

    if next_size == 0 {
        return Ok(Vec::new());
    }

    let mut encoded_block = vec![0u8; next_size as usize];
    let size = encoded_block.len();

    unsafe {
        host::consume_next_txs(
            Ptr32Mut::from_slice(encoded_block.as_mut_slice()),
            u32::try_from(size).expect("We only support wasm32 target; qed"),
        )
    };

    let txs: Vec<Transaction> = postcard::from_bytes(&encoded_block)
        .map_err(|e| anyhow::anyhow!("Failed to decode the transactions: {:?}", e))?;

    Ok(txs
        .into_iter()
        .map(MaybeCheckedTransaction::Transaction)
        .collect())
}

/// Gets the size of the value under the `key` in the `column` from the storage.
pub fn size_of_value(key: &[u8], column: u32) -> anyhow::Result<Option<usize>> {
    let val = unsafe {
        host::storage_size_of_value(
            Ptr32::from_slice(key),
            u32::try_from(key.len()).expect("We only support wasm32 target; qed"),
            column,
        )
    };

    let (exists, size, result) = unpack_exists_size_result(val);

    if result != 0 {
        return Err(anyhow::anyhow!(
            "Failed to get the size of the value from the WASM storage"
        ));
    }

    if exists {
        Ok(Some(size as usize))
    } else {
        Ok(None)
    }
}

/// Gets the value under the `key` in the `column` from the storage and copies it to the `out`.
pub fn get(key: &[u8], column: u32, out: &mut [u8]) -> anyhow::Result<()> {
    let output_size = out.len();
    let result = unsafe {
        host::storage_get(
            Ptr32::from_slice(key),
            u32::try_from(key.len()).expect("We only support wasm32 target; qed"),
            column,
            Ptr32Mut::from_slice(out),
            u32::try_from(output_size).expect("We only support wasm32 target; qed"),
        )
    };

    if result.0 != 0 {
        return Err(anyhow::anyhow!(
            "Failed to get the value from the WASM storage"
        ));
    }

    Ok(())
}

/// Returns `true` if the relayer is enabled. Uses a host function.
pub fn relayer_enabled() -> bool {
    unsafe { host::relayer_enabled() }
}

/// Gets the events at the `da_block_height` from the relayer by using host functions.
pub fn relayer_get_events(da_block_height: DaBlockHeight) -> anyhow::Result<Vec<Event>> {
    let size = unsafe { host::relayer_size_of_events(da_block_height.into()) };
    let (exists, size, result) = unpack_exists_size_result(size);

    if result != 0 {
        return Err(anyhow::anyhow!(
            "Failed to get the size of events from the WASM relayer"
        ));
    }

    if !exists || size == 0 {
        return Ok(vec![]);
    }

    let mut encoded_events = vec![0u8; size as usize];

    unsafe {
        host::relayer_get_events(
            da_block_height.into(),
            Ptr32Mut::from_slice(encoded_events.as_mut_slice()),
        )
    };

    let events: Vec<Event> = postcard::from_bytes(&encoded_events)
        .map_err(|e| anyhow::anyhow!("Failed to decode the events: {:?}", e))?;

    Ok(events)
}
