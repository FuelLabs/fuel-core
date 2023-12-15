use super::gas::GasCounter;
use super::{padding_to_16, DeterministicHostError, HostExportError};

use super::{AscHeap, AscIndexId, AscType, IndexForAscTypeId};
use semver::Version;
use std::fmt;
use std::marker::PhantomData;
use std::mem::MaybeUninit;

/// The `rt_size` field contained in an AssemblyScript header has a size of 4 bytes.
const SIZE_OF_RT_SIZE: u32 = 4;

/// A pointer to an object in the Asc heap.
pub struct AscPtr<C>(u32, PhantomData<C>);

impl<T> Copy for AscPtr<T> {}

impl<T> Clone for AscPtr<T> {
    fn clone(&self) -> Self {
        AscPtr(self.0, PhantomData)
    }
}

impl<T> Default for AscPtr<T> {
    fn default() -> Self {
        AscPtr(0, PhantomData)
    }
}

impl<T> fmt::Debug for AscPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<C> AscPtr<C> {
    /// A raw pointer to be passed to Wasm.
    pub fn wasm_ptr(self) -> u32 {
        self.0
    }

    #[inline(always)]
    pub fn new(heap_ptr: u32) -> Self {
        Self(heap_ptr, PhantomData)
    }
}

impl<C: AscType> AscPtr<C> {
    /// Create a pointer that is equivalent to AssemblyScript's `null`.
    #[inline(always)]
    pub fn null() -> Self {
        AscPtr::new(0)
    }

    /// Read from `self` into the Rust struct `C`.
    pub fn read_ptr<H: AscHeap + ?Sized>(
        self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<C, DeterministicHostError> {
        let len = match heap.api_version() {
            // TODO: The version check here conflicts with the comment on C::asc_size,
            // which states "Only used for version <= 0.0.3."
            version if version <= Version::new(0, 0, 4) => C::asc_size(self, heap, gas),
            _ => self.read_len(heap, gas),
        }?;

        let using_buffer = |buffer: &mut [MaybeUninit<u8>]| {
            let buffer = heap.read(self.0, buffer, gas)?;
            C::from_asc_bytes(buffer, &heap.api_version())
        };

        let len = len as usize;

        if len <= 32 {
            let mut buffer = [MaybeUninit::<u8>::uninit(); 32];
            using_buffer(&mut buffer[..len])
        } else {
            let mut buffer = Vec::with_capacity(len);
            using_buffer(buffer.spare_capacity_mut())
        }
    }

    /// Allocate `asc_obj` as an Asc object of class `C`.
    pub fn alloc_obj<H: AscHeap + ?Sized>(
        asc_obj: C,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<C>, HostExportError>
    where
        C: AscIndexId,
    {
        match heap.api_version() {
            version if version <= Version::new(0, 0, 4) => {
                let heap_ptr = heap.raw_new(&asc_obj.to_asc_bytes()?, gas)?;
                Ok(AscPtr::new(heap_ptr))
            }
            _ => {
                let mut bytes = asc_obj.to_asc_bytes()?;

                let aligned_len = padding_to_16(bytes.len());
                // Since AssemblyScript keeps all allocated objects with a 16 byte alignment,
                // we need to do the same when we allocate ourselves.
                bytes.extend(std::iter::repeat(0).take(aligned_len));

                let header = Self::generate_header(
                    heap,
                    C::INDEX_ASC_TYPE_ID,
                    asc_obj.content_len(&bytes),
                    bytes.len(),
                )?;
                let header_len = header.len() as u32;

                let heap_ptr = heap.raw_new(&[header, bytes].concat(), gas)?;

                // Use header length as offset. so the AscPtr points directly at the content.
                Ok(AscPtr::new(heap_ptr + header_len))
            }
        }
    }

    /// Helper used by arrays and strings to read their length.
    /// Only used for version <= 0.0.4.
    pub fn read_u32<H: AscHeap + ?Sized>(
        &self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<u32, DeterministicHostError> {
        // Read the bytes pointed to by `self` as the bytes of a `u32`.
        heap.read_u32(self.0, gas)
    }

    /// Helper that generates an AssemblyScript header.
    /// An AssemblyScript header has 20 bytes and it is composed of 5 values.
    /// - mm_info: usize -> size of all header contents + payload contents + padding
    /// - gc_info: usize -> first GC info (we don't free memory so it's irrelevant)
    /// - gc_info2: usize -> second GC info (we don't free memory so it's irrelevant)
    /// - rt_id: u32 -> identifier for the class being allocated
    /// - rt_size: u32 -> content size
    /// Only used for version >= 0.0.5.
    fn generate_header<H: AscHeap + ?Sized>(
        heap: &mut H,
        type_id_index: IndexForAscTypeId,
        content_length: usize,
        full_length: usize,
    ) -> Result<Vec<u8>, HostExportError> {
        let mut header: Vec<u8> = Vec::with_capacity(20);

        let gc_info: [u8; 4] = (0u32).to_le_bytes();
        let gc_info2: [u8; 4] = (0u32).to_le_bytes();
        let asc_type_id = heap.asc_type_id(type_id_index)?;
        let rt_id: [u8; 4] = asc_type_id.to_le_bytes();
        let rt_size: [u8; 4] = (content_length as u32).to_le_bytes();

        let mm_info: [u8; 4] =
            ((gc_info.len() + gc_info2.len() + rt_id.len() + rt_size.len() + full_length) as u32)
                .to_le_bytes();

        header.extend(mm_info);
        header.extend(gc_info);
        header.extend(gc_info2);
        header.extend(rt_id);
        header.extend(rt_size);

        Ok(header)
    }

    /// Helper to read the length from the header.
    /// An AssemblyScript header has 20 bytes, and it's right before the content, and composed by:
    /// - mm_info: usize
    /// - gc_info: usize
    /// - gc_info2: usize
    /// - rt_id: u32
    /// - rt_size: u32
    /// This function returns the `rt_size`.
    /// Only used for version >= 0.0.5.
    pub fn read_len<H: AscHeap + ?Sized>(
        &self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<u32, DeterministicHostError> {
        // We're trying to read the pointer below, we should check it's
        // not null before using it.
        self.check_is_not_null()?;

        let start_of_rt_size = self.0.checked_sub(SIZE_OF_RT_SIZE).ok_or_else(|| {
            DeterministicHostError::from(anyhow::anyhow!(
                "Subtract overflow on pointer: {}",
                self.0
            ))
        })?;

        heap.read_u32(start_of_rt_size, gas)
    }

    /// Conversion to `u64` for use with `AscEnum`.
    pub fn to_payload(&self) -> u64 {
        self.0 as u64
    }

    /// We typically assume `AscPtr` is never null, but for types such as `string | null` it can be.
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    /// There's no problem in an AscPtr being 'null' (see above AscPtr::is_null function).
    /// However if one tries to read that pointer, it should fail with a helpful error message,
    /// this function does this error handling.
    ///
    /// Summary: ALWAYS call this before reading an AscPtr.
    pub fn check_is_not_null(&self) -> Result<(), DeterministicHostError> {
        if self.is_null() {
            return Err(DeterministicHostError::from(anyhow::anyhow!(
                "Tried to read AssemblyScript value that is 'null'. Suggestion: look into the function that the error happened and add 'log' calls till you find where a 'null' value is being used as non-nullable. It's likely that you're calling a 'graph-ts' function (or operator) with a 'null' value when it doesn't support it."
            )));
        }

        Ok(())
    }

    // Erase type information.
    pub fn erase(self) -> AscPtr<()> {
        AscPtr::new(self.0)
    }
}

impl<C> From<u32> for AscPtr<C> {
    fn from(ptr: u32) -> Self {
        AscPtr::new(ptr)
    }
}

impl<T> AscType for AscPtr<T> {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        let bytes = u32::from_asc_bytes(asc_obj, api_version)?;
        Ok(AscPtr::new(bytes))
    }
}
