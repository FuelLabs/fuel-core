use std::marker::PhantomData;
use std::mem::{size_of, size_of_val};

use anyhow::anyhow;
use semver::Version;

use graph::runtime::gas::GasCounter;
use graph::runtime::{
    AscHeap, AscPtr, AscType, AscValue, DeterministicHostError, HostExportError, HEADER_SIZE,
};
use graph_runtime_derive::AscType;

use crate::asc_abi::class;

/// Module related to AssemblyScript version >=v0.19.2.
/// All `to_asc_bytes`/`from_asc_bytes` only consider the #data/content/payload
/// not the #header, that's handled on `AscPtr`.
/// Header in question: https://www.assemblyscript.org/memory.html#common-header-layout

/// Similar as JS ArrayBuffer, "a generic, fixed-length raw binary data buffer".
/// See https://www.assemblyscript.org/memory.html#arraybuffer-layout
pub struct ArrayBuffer {
    // Not included in memory layout
    pub byte_length: u32,
    // #data
    pub content: Box<[u8]>,
}

impl ArrayBuffer {
    pub fn new<T: AscType>(values: &[T]) -> Result<Self, DeterministicHostError> {
        let mut content = Vec::new();
        for value in values {
            let asc_bytes = value.to_asc_bytes()?;
            content.extend(&asc_bytes);
        }

        if content.len() > u32::max_value() as usize {
            return Err(DeterministicHostError::from(anyhow::anyhow!(
                "slice cannot fit in WASM memory"
            )));
        }
        Ok(ArrayBuffer {
            byte_length: content.len() as u32,
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
    }
}

impl AscType for ArrayBuffer {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        let mut asc_layout: Vec<u8> = Vec::new();

        asc_layout.extend(self.content.iter());

        // Allocate extra capacity to next power of two, as required by asc.
        let total_size = self.byte_length as usize + HEADER_SIZE;
        let total_capacity = total_size.next_power_of_two();
        let extra_capacity = total_capacity - total_size;
        asc_layout.extend(std::iter::repeat(0).take(extra_capacity));

        Ok(asc_layout)
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        _api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        Ok(ArrayBuffer {
            byte_length: asc_obj.len() as u32,
            content: asc_obj.to_vec().into(),
        })
    }

    fn content_len(&self, _asc_bytes: &[u8]) -> usize {
        self.byte_length as usize // without extra_capacity
    }
}

/// A typed, indexable view of an `ArrayBuffer` of Asc primitives. In Asc it's
/// an abstract class with subclasses for each primitive, for example
/// `Uint8Array` is `TypedArray<u8>`.
/// Also known as `ArrayBufferView`.
/// See https://www.assemblyscript.org/memory.html#arraybufferview-layout
#[repr(C)]
#[derive(AscType)]
pub struct TypedArray<T> {
    // #data -> Backing buffer reference
    pub buffer: AscPtr<ArrayBuffer>,
    // #dataStart -> Start within the #data
    data_start: u32,
    // #dataLength -> Length of the data from #dataStart
    byte_length: u32,
    // Not included in memory layout, it's just for typings
    ty: PhantomData<T>,
}

impl<T: AscValue> TypedArray<T> {
    pub(crate) fn new<H: AscHeap + ?Sized>(
        content: &[T],
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<Self, HostExportError> {
        let buffer = class::ArrayBuffer::new(content, heap.api_version())?;
        let byte_length = content.len() as u32;
        let ptr = AscPtr::alloc_obj(buffer, heap, gas)?;
        Ok(TypedArray {
            buffer: AscPtr::new(ptr.wasm_ptr()), // new AscPtr necessary to convert type parameter
            data_start: ptr.wasm_ptr(),
            byte_length,
            ty: PhantomData,
        })
    }

    pub(crate) fn to_vec<H: AscHeap + ?Sized>(
        &self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<Vec<T>, DeterministicHostError> {
        // We're trying to read the pointer below, we should check it's
        // not null before using it.
        self.buffer.check_is_not_null()?;

        // This subtraction is needed because on the ArrayBufferView memory layout
        // there are two pointers to the data.
        // - The first (self.buffer) points to the related ArrayBuffer.
        // - The second (self.data_start) points to where in this ArrayBuffer the data starts.
        // So this is basically getting the offset.
        // Related docs: https://www.assemblyscript.org/memory.html#arraybufferview-layout
        let data_start_with_offset = self
            .data_start
            .checked_sub(self.buffer.wasm_ptr())
            .ok_or_else(|| {
                DeterministicHostError::from(anyhow::anyhow!(
                    "Subtract overflow on pointer: {}",
                    self.data_start
                ))
            })?;

        self.buffer.read_ptr(heap, gas)?.get(
            data_start_with_offset,
            self.byte_length / size_of::<T>() as u32,
            heap.api_version(),
        )
    }
}

/// Asc std string: "Strings are encoded as UTF-16LE in AssemblyScript"
/// See https://www.assemblyscript.org/memory.html#string-layout
pub struct AscString {
    // Not included in memory layout
    // In number of UTF-16 code units (2 bytes each).
    byte_length: u32,
    // #data
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
            byte_length: content.len() as u32,
            content: content.into(),
        })
    }
}

impl AscType for AscString {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        let mut content: Vec<u8> = Vec::new();

        // Write the code points, in little-endian (LE) order.
        for &code_unit in self.content.iter() {
            let low_byte = code_unit as u8;
            let high_byte = (code_unit >> 8) as u8;
            content.push(low_byte);
            content.push(high_byte);
        }

        let header_size = 20;
        let total_size = (self.byte_length as usize * 2) + header_size;
        let total_capacity = total_size.next_power_of_two();
        let extra_capacity = total_capacity - total_size;
        content.extend(std::iter::repeat(0).take(extra_capacity));

        Ok(content)
    }

    /// The Rust representation of an Asc object as layed out in Asc memory.
    fn from_asc_bytes(
        asc_obj: &[u8],
        _api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        // UTF-16 (used in assemblyscript) always uses one
        // pair of bytes per code unit.
        // https://mathiasbynens.be/notes/javascript-encoding
        // UTF-16 (16-bit Unicode Transformation Format) is an
        // extension of UCS-2 that allows representing code points
        // outside the BMP. It produces a variable-length result
        // of either one or two 16-bit code units per code point.
        // This way, it can encode code points in the range from 0
        // to 0x10FFFF.

        let mut content = Vec::new();
        for pair in asc_obj.chunks(2) {
            let code_point_bytes = [
                pair[0],
                *pair.get(1).ok_or_else(|| {
                    DeterministicHostError::from(anyhow!(
                        "Attempted to read past end of string content bytes chunk"
                    ))
                })?,
            ];
            let code_point = u16::from_le_bytes(code_point_bytes);
            content.push(code_point);
        }
        AscString::new(&content)
    }

    fn content_len(&self, _asc_bytes: &[u8]) -> usize {
        self.byte_length as usize * 2 // without extra_capacity, and times 2 because the content is measured in u8s
    }
}

/// Growable array backed by an `ArrayBuffer`.
/// See https://www.assemblyscript.org/memory.html#array-layout
#[repr(C)]
#[derive(AscType)]
pub struct Array<T> {
    // #data -> Backing buffer reference
    buffer: AscPtr<ArrayBuffer>,
    // #dataStart -> Start of the data within #data
    buffer_data_start: u32,
    // #dataLength -> Length of the data from #dataStart
    buffer_data_length: u32,
    // #length -> Mutable length of the data the user is interested in
    length: i32,
    // Not included in memory layout, it's just for typings
    ty: PhantomData<T>,
}

impl<T: AscValue> Array<T> {
    pub fn new<H: AscHeap + ?Sized>(
        content: &[T],
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<Self, HostExportError> {
        let arr_buffer = class::ArrayBuffer::new(content, heap.api_version())?;
        let buffer = AscPtr::alloc_obj(arr_buffer, heap, gas)?;
        let buffer_data_length = buffer.read_len(heap, gas)?;
        Ok(Array {
            buffer: AscPtr::new(buffer.wasm_ptr()),
            buffer_data_start: buffer.wasm_ptr(),
            buffer_data_length,
            length: content.len() as i32,
            ty: PhantomData,
        })
    }

    pub(crate) fn to_vec<H: AscHeap + ?Sized>(
        &self,
        heap: &H,
        gas: &GasCounter,
    ) -> Result<Vec<T>, DeterministicHostError> {
        // We're trying to read the pointer below, we should check it's
        // not null before using it.
        self.buffer.check_is_not_null()?;

        // This subtraction is needed because on the ArrayBufferView memory layout
        // there are two pointers to the data.
        // - The first (self.buffer) points to the related ArrayBuffer.
        // - The second (self.buffer_data_start) points to where in this ArrayBuffer the data starts.
        // So this is basically getting the offset.
        // Related docs: https://www.assemblyscript.org/memory.html#arraybufferview-layout
        let buffer_data_start_with_offset = self
            .buffer_data_start
            .checked_sub(self.buffer.wasm_ptr())
            .ok_or_else(|| {
                DeterministicHostError::from(anyhow::anyhow!(
                    "Subtract overflow on pointer: {}",
                    self.buffer_data_start
                ))
            })?;

        self.buffer.read_ptr(heap, gas)?.get(
            buffer_data_start_with_offset,
            self.length as u32,
            heap.api_version(),
        )
    }
}
