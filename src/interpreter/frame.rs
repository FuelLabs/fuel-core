use super::{Interpreter, MemoryRange};
use crate::consts::*;

use fuel_asm::Word;
use fuel_tx::{bytes, Address, Color};

use std::{io, mem};

const ADDRESS_SIZE: usize = mem::size_of::<Address>();
const COLOR_SIZE: usize = mem::size_of::<Color>();
const WORD_SIZE: usize = mem::size_of::<Word>();

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Call {
    to: Address,
    inputs: Vec<MemoryRange>,
    outputs: Vec<MemoryRange>,
}

impl Call {
    pub const fn new(to: Address, inputs: Vec<MemoryRange>, outputs: Vec<MemoryRange>) -> Self {
        Self { to, inputs, outputs }
    }

    pub fn has_outputs_ownership(&self, vm: &Interpreter) -> bool {
        self.outputs
            .iter()
            .fold(true, |acc, range| acc && vm.has_ownership_range(range))
    }

    pub const fn to(&self) -> &Address {
        &self.to
    }

    pub fn inputs(&self) -> &[MemoryRange] {
        self.inputs.as_slice()
    }

    pub fn outputs(&self) -> &[MemoryRange] {
        self.outputs.as_slice()
    }

    pub fn into_inner(self) -> (Address, Vec<MemoryRange>, Vec<MemoryRange>) {
        (self.to, self.inputs, self.outputs)
    }
}

impl io::Read for Call {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = ADDRESS_SIZE + 2 * WORD_SIZE * (1 + self.outputs.len() + self.inputs.len());
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
        let mut n = ADDRESS_SIZE + 2 * WORD_SIZE;
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
    to: Address,
    color: Color,
    registers: [Word; VM_REGISTER_COUNT],
    inputs: Vec<MemoryRange>,
    outputs: Vec<MemoryRange>,
    code: Vec<u8>,
}

impl CallFrame {
    pub const fn new(
        to: Address,
        color: Color,
        registers: [Word; VM_REGISTER_COUNT],
        inputs: Vec<MemoryRange>,
        outputs: Vec<MemoryRange>,
        code: Vec<u8>,
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
        self.code.as_slice()
    }
}

impl io::Read for CallFrame {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut n = ADDRESS_SIZE
            + COLOR_SIZE
            + WORD_SIZE * (3 + VM_REGISTER_COUNT + 2 * (self.inputs.len() + self.outputs.len()));
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
        let buf = bytes::store_number_unchecked(buf, self.code.len() as Word);

        let buf = self.inputs.iter().chain(self.outputs.iter()).fold(buf, |buf, range| {
            let buf = bytes::store_number_unchecked(buf, range.start());
            bytes::store_number_unchecked(buf, range.len())
        });

        let (bytes, _) = bytes::store_raw_bytes(buf, self.code.as_slice())?;
        n += bytes;

        Ok(n)
    }
}

impl io::Write for CallFrame {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut n = ADDRESS_SIZE
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
        self.code = code;

        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Interpreter {
    pub fn call_frame(&self, call: Call, color: Color) -> CallFrame {
        let (to, inputs, outputs) = call.into_inner();

        // TODO fetch code
        let code = vec![0xcd; 256];
        let registers = self.registers.clone();

        CallFrame::new(to, color, registers, inputs, outputs, code)
    }
}
