use super::{Interpreter, MemoryRange};
use crate::bytes;
use crate::consts::*;
use crate::transaction::{Color, Id};
use crate::types::Word;

use std::{io, mem};

const ID_SIZE: usize = mem::size_of::<Id>();
const WORD_SIZE: usize = mem::size_of::<Word>();

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Call {
    to: Id,
    inputs: Vec<MemoryRange>,
    outputs: Vec<MemoryRange>,
}

impl Call {
    pub const fn new(to: Id, inputs: Vec<MemoryRange>, outputs: Vec<MemoryRange>) -> Self {
        Self { to, inputs, outputs }
    }

    pub fn has_outputs_ownership(&self, vm: &Interpreter) -> bool {
        self.outputs
            .iter()
            .fold(true, |acc, range| acc && vm.has_ownership_range(range))
    }
}

impl io::Read for Call {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = ID_SIZE + 2 * WORD_SIZE * (1 + self.outputs.len() + self.inputs.len());
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
        let mut n = ID_SIZE + 2 * WORD_SIZE;
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

        let mut outputs = (0..outputs_len + inputs_len)
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
    to: Id,
    color: Color,
    registers: [Word; VM_REGISTER_COUNT],
    code_size: u8,
    inputs: Vec<MemoryRange>,
    outputs: Vec<MemoryRange>,
    code: Vec<u8>,
    stack: Vec<u8>,
}

impl CallFrame {
    pub const fn new(
        to: Id,
        color: Color,
        registers: [Word; VM_REGISTER_COUNT],
        code_size: u8,
        inputs: Vec<MemoryRange>,
        outputs: Vec<MemoryRange>,
        code: Vec<u8>,
        stack: Vec<u8>,
    ) -> Self {
        Self {
            to,
            color,
            registers,
            code_size,
            inputs,
            outputs,
            code,
            stack,
        }
    }
}
