use graph::runtime::gas::GasCounter;
use std::convert::TryInto as _;
use std::marker::PhantomData;
use std::mem::{size_of, size_of_val};

use anyhow::anyhow;
use semver::Version;

use graph::runtime::{AscHeap, AscPtr, AscType, AscValue, DeterministicHostError, HostExportError};
use graph_runtime_derive::AscType;

use crate::asc_abi::class;

/// Module related to AssemblyScript version v0.6.

/// Asc std ArrayBuffer: "a generic, fixed-length raw binary data buffer".
/// See https://github.com/AssemblyScript/assemblyscript/wiki/Memory-Layout-&-Management/86447e88be5aa8ec633eaf5fe364651136d136ab#arrays
pub struct ArrayBuffer {
    pub byte_length: u32,
    // Asc allocators always align at 8 bytes, we already have 4 bytes from
    // `byte_length_size` so with 4 more bytes we align the contents at 8
    // bytes. No Asc type has alignment greater than 8, so the
    // elements in `content` will be aligned for any element type.
    pub padding: [u8; 4],
    // In Asc this slice is layed out inline with the ArrayBuffer.
    pub content: Box<[u8]>,
}

impl ArrayBuffer {
    pub fn new<T: AscType>(values: &[T]) -> Result<Self, DeterministicHostError> {
        let mut content = Vec::new();
        for value in values {
            let asc_bytes = value.to_asc_bytes()?;
            // An `AscValue` has size equal to alignment, no padding required.
            content.extend(&asc_bytes);
        }

        if content.len() > u32::max_value() as usize {
            return Err(DeterministicHostError::from(anyhow::anyhow!(
                "slice cannot fit in WASM memory"
            )));
        }
        Ok(ArrayBuffer {
            byte_length: content.len() as u32,
            padding: [0; 4],
            content: content.into(),
        })
    }

    /// Read `length` elements of type `T` starting at `byte_offset`.
    ///
    /// Panics if that tries to read beyond the length of `self.content`.
    pub fn get<T: AscType>(
        &self,
        byte_offset: u32,
        length: u32,
        api_version: Version,
    ) -> Result<Vec<T>, DeterministicHostError> {
        let length = length as usize;
        let byte_offset = byte_offset as usize;

        self.content[byte_offset..]
            .chunks(size_of::<T>())
            .take(length)
            .map(|asc_obj| T::from_asc_bytes(asc_obj, &api_version))
            .collect()

        // TODO: This code is preferred as it validates the length of the array.
        // But, some existing subgraphs were found to break when this was added.
        // This needs to be root caused
        /*
        let range = byte_offset..byte_offset + length * size_of::<T>();
        self.content
            .get(range)
            .ok_or_else(|| {
                DeterministicHostError::from(anyhow::anyhow!("Attempted to read past end of array"))
            })?
            .chunks_exact(size_of::<T>())
            .map(|bytes| T::from_asc_bytes(bytes))
            .collect()
            */
    }
}

impl AscType for ArrayBuffer {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        let mut asc_layout: Vec<u8> = Vec::new();

        let byte_length: [u8; 4] = self.byte_length.to_le_bytes();
        asc_layout.extend(byte_length);
        asc_layout.extend(self.padding);
        asc_layout.extend(self.content.iter());

        // Allocate extra capacity to next power of two, as required by asc.
        let header_size = size_of_val(&byte_length) + size_of_val(&self.padding);
        let total_size = self.byte_length as usize + header_size;
        let total_capacity = total_size.next_power_of_two();
        let extra_capacity = total_capacity - total_size;
        asc_layout.extend(std::iter::repeat(0).take(extra_capacity));
        assert_eq!(asc_layout.len(), total_capacity);

        Ok(asc_layout)
    }

    /// The Rust representation of an Asc object as layed out in Asc memory.
    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        // Skip `byte_length` and the padding.
        let content_offset = size_of::<u32>() + 4;
        let byte_length = asc_obj.get(..size_of::<u32>()).ok_or_else(|| {
            DeterministicHostError::from(anyhow!("Attempted to read past end of array"))
        })?;
        let content = asc_obj.get(content_offset..).ok_or_else(|| {
            DeterministicHostError::from(anyhow!("Attempted to read past end of array"))
        })?;
        Ok(ArrayBuffer {
            byte_length: u32::from_asc_bytes(byte_length, api_version)?,
            padding: [0; 4],
            content: content.to_vec().into(),
        })
    }

    fn asc_size<H: AscHeap + ?Sized>(
        ptr: AscPtr<Self>,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<u32, DeterministicHostError> {
        let byte_length = ptr.read_u32(heap, gas)?;
        let byte_length_size = size_of::<u32>() as u32;
        let padding_size = size_of::<u32>() as u32;
        Ok(byte_length_size + padding_size + byte_length)
    }
}

/// A typed, indexable view of an `ArrayBuffer` of Asc primitives. In Asc it's
/// an abstract class with subclasses for each primitive, for example
/// `Uint8Array` is `TypedArray<u8>`.
///  See https://github.com/AssemblyScript/assemblyscript/wiki/Memory-Layout-&-Management/86447e88be5aa8ec633eaf5fe364651136d136ab#arrays
#[repr(C)]
#[derive(AscType)]
pub struct TypedArray<T> {
    pub buffer: AscPtr<ArrayBuffer>,
    /// Byte position in `buffer` of the array start.
    byte_offset: u32,
    byte_length: u32,
    ty: PhantomData<T>,
}

impl<T: AscValue> TypedArray<T> {
    pub(crate) fn new<H: AscHeap + ?Sized>(
        content: &[T],
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<Self, HostExportError> {
        let buffer = class::ArrayBuffer::new(content, heap.api_version())?;
        let buffer_byte_length = if let class::ArrayBuffer::ApiVersion0_0_4(ref a) = buffer {
            a.byte_length
        } else {
            unreachable!("Only the correct ArrayBuffer will be constructed")
        };
        let ptr = AscPtr::alloc_obj(buffer, heap, gas)?;
        Ok(TypedArray {
            byte_length: buffer_byte_length,
            buffer: AscPtr::new(ptr.wasm_ptr()),
            byte_offset: 0,
            ty: PhantomData,
        })
    }

    pub(crate) fn to_vec<H: AscHeap + ?Sized>(
        &self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<Vec<T>, DeterministicHostError> {
        self.buffer.read_ptr(heap, gas)?.get(
            self.byte_offset,
            self.byte_length / size_of::<T>() as u32,
            heap.api_version(),
        )
    }
}

/// Asc std string: "Strings are encoded as UTF-16LE in AssemblyScript, and are
/// prefixed with their length (in character codes) as a 32-bit integer". See
/// https://github.com/AssemblyScript/assemblyscript/wiki/Memory-Layout-&-Management/86447e88be5aa8ec633eaf5fe364651136d136ab#arrays
pub struct AscString {
    // In number of UTF-16 code units (2 bytes each).
    length: u32,
    // The sequence of UTF-16LE code units that form the string.
    pub content: Box<[u16]>,
}

impl AscString {
    pub fn new(content: &[u16]) -> Result<Self, DeterministicHostError> {
        if size_of_val(content) > u32::max_value() as usize {
            return Err(DeterministicHostError::from(anyhow!(
                "string cannot fit in WASM memory"
            )));
        }

        Ok(AscString {
            length: content.len() as u32,
            content: content.into(),
        })
    }
}

impl AscType for AscString {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        let mut asc_layout: Vec<u8> = Vec::new();

        let length: [u8; 4] = self.length.to_le_bytes();
        asc_layout.extend(length);

        // Write the code points, in little-endian (LE) order.
        for &code_unit in self.content.iter() {
            let low_byte = code_unit as u8;
            let high_byte = (code_unit >> 8) as u8;
            asc_layout.push(low_byte);
            asc_layout.push(high_byte);
        }

        Ok(asc_layout)
    }

    /// The Rust representation of an Asc object as layed out in Asc memory.
    fn from_asc_bytes(
        asc_obj: &[u8],
        _api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        // Pointer for our current position within `asc_obj`,
        // initially at the start of the content skipping `length`.
        let mut offset = size_of::<i32>();

        let length = asc_obj
            .get(..offset)
            .ok_or(DeterministicHostError::from(anyhow::anyhow!(
                "String bytes not long enough to contain length"
            )))?;

        // Does not panic - already validated slice length == size_of::<i32>.
        let length = i32::from_le_bytes(length.try_into().unwrap());
        if length.checked_mul(2).and_then(|l| l.checked_add(4)) != asc_obj.len().try_into().ok() {
            return Err(DeterministicHostError::from(anyhow::anyhow!(
                "String length header does not equal byte length"
            )));
        }

        // Prevents panic when accessing offset + 1 in the loop
        if asc_obj.len() % 2 != 0 {
            return Err(DeterministicHostError::from(anyhow::anyhow!(
                "Invalid string length"
            )));
        }

        // UTF-16 (used in assemblyscript) always uses one
        // pair of bytes per code unit.
        // https://mathiasbynens.be/notes/javascript-encoding
        // UTF-16 (16-bit Unicode Transformation Format) is an
        // extension of UCS-2 that allows representing code points
        // outside the BMP. It produces a variable-length result
        // of either one or two 16-bit code units per code point.
        // This way, it can encode code points in the range from 0
        // to 0x10FFFF.

        // Read the content.
        let mut content = Vec::new();
        while offset < asc_obj.len() {
            let code_point_bytes = [asc_obj[offset], asc_obj[offset + 1]];
            let code_point = u16::from_le_bytes(code_point_bytes);
            content.push(code_point);
            offset += size_of::<u16>();
        }
        AscString::new(&content)
    }

    fn asc_size<H: AscHeap + ?Sized>(
        ptr: AscPtr<Self>,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<u32, DeterministicHostError> {
        let length = ptr.read_u32(heap, gas)?;
        let length_size = size_of::<u32>() as u32;
        let code_point_size = size_of::<u16>() as u32;
        let data_size = code_point_size.checked_mul(length);
        let total_size = data_size.and_then(|d| d.checked_add(length_size));
        total_size.ok_or_else(|| {
            DeterministicHostError::from(anyhow::anyhow!("Overflowed when getting size of string"))
        })
    }
}

/// Growable array backed by an `ArrayBuffer`.
/// See https://github.com/AssemblyScript/assemblyscript/wiki/Memory-Layout-&-Management/86447e88be5aa8ec633eaf5fe364651136d136ab#arrays
#[repr(C)]
#[derive(AscType)]
pub struct Array<T> {
    buffer: AscPtr<ArrayBuffer>,
    length: u32,
    ty: PhantomData<T>,
}

impl<T: AscValue> Array<T> {
    pub fn new<H: AscHeap + ?Sized>(
        content: &[T],
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<Self, HostExportError> {
        let arr_buffer = class::ArrayBuffer::new(content, heap.api_version())?;
        let arr_buffer_ptr = AscPtr::alloc_obj(arr_buffer, heap, gas)?;
        Ok(Array {
            buffer: AscPtr::new(arr_buffer_ptr.wasm_ptr()),
            // If this cast would overflow, the above line has already panicked.
            length: content.len() as u32,
            ty: PhantomData,
        })
    }

    pub(crate) fn to_vec<H: AscHeap + ?Sized>(
        &self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<Vec<T>, DeterministicHostError> {
        self.buffer
            .read_ptr(heap, gas)?
            .get(0, self.length, heap.api_version())
    }
}
