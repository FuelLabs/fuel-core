use super::{Contract, ExecuteError, Interpreter, MemoryRange};
use crate::consts::*;

use fuel_asm::Word;
use fuel_tx::bytes::SizedBytes;
use fuel_tx::{bytes, Color, ContractAddress};

use std::{io, mem};

const CONTRACT_ADDRESS_SIZE: usize = mem::size_of::<ContractAddress>();
const COLOR_SIZE: usize = mem::size_of::<Color>();
const WORD_SIZE: usize = mem::size_of::<Word>();

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Call {
    to: ContractAddress,
    inputs: Vec<MemoryRange>,
    outputs: Vec<MemoryRange>,
}

impl Call {
    pub const fn new(to: ContractAddress, inputs: Vec<MemoryRange>, outputs: Vec<MemoryRange>) -> Self {
        Self { to, inputs, outputs }
    }

    pub fn has_outputs_ownership(&self, vm: &Interpreter) -> bool {
        self.outputs
            .iter()
            .fold(true, |acc, range| acc && vm.has_ownership_range(range))
    }

    pub const fn to(&self) -> &ContractAddress {
        &self.to
    }

    pub fn inputs(&self) -> &[MemoryRange] {
        self.inputs.as_slice()
    }

    pub fn outputs(&self) -> &[MemoryRange] {
        self.outputs.as_slice()
    }

    pub fn into_inner(self) -> (ContractAddress, Vec<MemoryRange>, Vec<MemoryRange>) {
        (self.to, self.inputs, self.outputs)
    }
}

impl SizedBytes for Call {
    fn serialized_size(&self) -> usize {
        CONTRACT_ADDRESS_SIZE + 2 * WORD_SIZE * (1 + self.outputs.len() + self.inputs.len())
    }
}

impl io::Read for Call {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.serialized_size();
        if buf.len() < n {
            return Err(bytes::eof());
        }

        let buf = bytes::store_array_unchecked(buf, &self.to);
        let buf = bytes::store_number_unchecked(buf, self.outputs.len() as Word);
        let buf = bytes::store_number_unchecked(buf, self.inputs.len() as Word);

        self.outputs.iter().chain(self.inputs.iter()).fold(buf, |buf, range| {
            let buf = bytes::store_number_unchecked(buf, range.start());
            bytes::store_number_unchecked(buf, range.len())
        });

        Ok(n)
    }
}

impl io::Write for Call {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut n = CONTRACT_ADDRESS_SIZE + 2 * WORD_SIZE;
        if buf.len() < n {
            return Err(bytes::eof());
        }

        let (to, buf) = bytes::restore_array_unchecked(buf);
        let (outputs_len, buf) = bytes::restore_word_unchecked(buf);
        let (inputs_len, mut buf) = bytes::restore_word_unchecked(buf);

        let size = 2 * WORD_SIZE * (outputs_len + inputs_len) as usize;
        if buf.len() < size {
            return Err(bytes::eof());
        }
        n += size;

        let mut outputs = (0..2 * (outputs_len + inputs_len))
            .map(|_| {
                let (n, b) = bytes::restore_word_unchecked(buf);
                buf = b;
                n
            })
            .collect::<Vec<Word>>()
            .as_slice()
            .chunks_exact(2)
            .map(|chunk| MemoryRange::new(chunk[0], chunk[1]))
            .collect::<Vec<MemoryRange>>();

        if outputs.len() as Word != outputs_len + inputs_len {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The inputs and outputs addresses doesn't correspond to the expected structure!",
            ))?;
        }

        self.to = to;
        self.inputs = outputs.split_off(outputs_len as usize);
        self.outputs = outputs;

        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallFrame {
    to: ContractAddress,
    color: Color,
    registers: [Word; VM_REGISTER_COUNT],
    inputs: Vec<MemoryRange>,
    outputs: Vec<MemoryRange>,
    code: Contract,
}

impl CallFrame {
    pub const fn new(
        to: ContractAddress,
        color: Color,
        registers: [Word; VM_REGISTER_COUNT],
        inputs: Vec<MemoryRange>,
        outputs: Vec<MemoryRange>,
        code: Contract,
    ) -> Self {
        Self {
            to,
            color,
            registers,
            inputs,
            outputs,
            code,
        }
    }

    pub fn code(&self) -> &[u8] {
        self.code.as_ref()
    }

    pub fn code_offset(&self) -> usize {
        CONTRACT_ADDRESS_SIZE
            + COLOR_SIZE
            + WORD_SIZE * (3 + VM_REGISTER_COUNT + 2 * (self.inputs.len() + self.outputs.len()))
    }

    pub const fn inputs_outputs_offset() -> usize {
        CONTRACT_ADDRESS_SIZE // To
            + COLOR_SIZE // Color
            + VM_REGISTER_COUNT * WORD_SIZE // Registers
            + WORD_SIZE // Inputs size
            + WORD_SIZE // Outputs size
            + WORD_SIZE // Code size
    }

    pub const fn registers(&self) -> &[Word] {
        &self.registers
    }
}

impl SizedBytes for CallFrame {
    fn serialized_size(&self) -> usize {
        CONTRACT_ADDRESS_SIZE
            + COLOR_SIZE
            + WORD_SIZE * (3 + VM_REGISTER_COUNT + 2 * (self.inputs.len() + self.outputs.len()))
            + bytes::padded_len(self.code.as_ref())
    }
}

impl io::Read for CallFrame {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.serialized_size();
        if buf.len() < n {
            return Err(bytes::eof());
        }

        let buf = bytes::store_array_unchecked(buf, &self.to);
        let buf = bytes::store_array_unchecked(buf, &self.color);
        let buf = self
            .registers
            .iter()
            .fold(buf, |buf, reg| bytes::store_number_unchecked(buf, *reg));

        let buf = bytes::store_number_unchecked(buf, self.inputs.len() as Word);
        let buf = bytes::store_number_unchecked(buf, self.outputs.len() as Word);
        let buf = bytes::store_number_unchecked(buf, self.code.as_ref().len() as Word);

        let buf = self.inputs.iter().chain(self.outputs.iter()).fold(buf, |buf, range| {
            let buf = bytes::store_number_unchecked(buf, range.start());
            bytes::store_number_unchecked(buf, range.len())
        });

        bytes::store_raw_bytes(buf, self.code.as_ref())?;

        Ok(n)
    }
}

impl io::Write for CallFrame {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut n = CONTRACT_ADDRESS_SIZE
            + COLOR_SIZE
            + WORD_SIZE * (3 + VM_REGISTER_COUNT + 2 * (self.inputs.len() + self.outputs.len()));
        if buf.len() < n {
            return Err(bytes::eof());
        }

        let (to, buf) = bytes::restore_array_unchecked(buf);
        let (color, buf) = bytes::restore_array_unchecked(buf);

        let buf = self.registers.iter_mut().fold(buf, |buf, reg| {
            let (r, buf) = bytes::restore_word_unchecked(buf);
            *reg = r;
            buf
        });

        let (inputs_len, buf) = bytes::restore_word_unchecked(buf);
        let (outputs_len, buf) = bytes::restore_word_unchecked(buf);
        let (code_len, mut buf) = bytes::restore_usize_unchecked(buf);

        let mut inputs = (0..2 * (inputs_len + outputs_len))
            .map(|_| {
                let (n, b) = bytes::restore_word_unchecked(buf);
                buf = b;
                n
            })
            .collect::<Vec<Word>>()
            .as_slice()
            .chunks_exact(2)
            .map(|chunk| MemoryRange::new(chunk[0], chunk[1]))
            .collect::<Vec<MemoryRange>>();

        if inputs.len() as Word != inputs_len + outputs_len {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The inputs and outputs addresses doesn't correspond to the expected structure!",
            ))?;
        }

        let (bytes, code, _) = bytes::restore_raw_bytes(buf, code_len)?;
        n += bytes;

        self.to = to;
        self.color = color;
        self.outputs = inputs.split_off(inputs_len as usize);
        self.inputs = inputs;
        self.code = code.into();

        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Interpreter {
    pub fn call_frame(&self, call: Call, color: Color) -> Result<CallFrame, ExecuteError> {
        let (to, inputs, outputs) = call.into_inner();

        let code = self.contract(&to).cloned().ok_or(ExecuteError::ContractNotFound)?;
        let registers = self.registers.clone();

        let frame = CallFrame::new(to, color, registers, inputs, outputs, code);

        Ok(frame)
    }
}
