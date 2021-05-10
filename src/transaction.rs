use crate::bytes;
use crate::types::Word;

use std::{io, mem};

mod types;

pub use types::{Color, Id, Input, Output, Root, Witness};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Transaction {
    Script {
        gas_price: Word,
        gas_limit: Word,
        maturity: u32,
        script: Vec<u8>,
        script_data: Vec<u8>,
        inputs: Vec<Input>,
        outputs: Vec<Output>,
        witnesses: Vec<Witness>,
    } = 0x00,

    Create {
        gas_price: Word,
        gas_limit: Word,
        maturity: u32,
        bytecode_length: u16,
        bytecode_witness_index: u8,
        salt: [u8; 32],
        static_contracts: Vec<Id>,
        inputs: Vec<Input>,
        outputs: Vec<Output>,
        witnesses: Vec<Witness>,
    } = 0x01,
}

impl Transaction {
    pub const fn script(
        gas_price: Word,
        gas_limit: Word,
        maturity: u32,
        script: Vec<u8>,
        script_data: Vec<u8>,
        inputs: Vec<Input>,
        outputs: Vec<Output>,
        witnesses: Vec<Witness>,
    ) -> Self {
        Self::Script {
            gas_price,
            gas_limit,
            maturity,
            script,
            script_data,
            inputs,
            outputs,
            witnesses,
        }
    }

    pub const fn create(
        gas_price: Word,
        gas_limit: Word,
        maturity: u32,
        bytecode_length: u16,
        bytecode_witness_index: u8,
        salt: [u8; 32],
        static_contracts: Vec<Id>,
        inputs: Vec<Input>,
        outputs: Vec<Output>,
        witnesses: Vec<Witness>,
    ) -> Self {
        Self::Create {
            gas_price,
            gas_limit,
            maturity,
            bytecode_length,
            bytecode_witness_index,
            salt,
            static_contracts,
            inputs,
            outputs,
            witnesses,
        }
    }

    pub const fn gas_price(&self) -> Word {
        match self {
            Self::Script { gas_price, .. } => *gas_price,
            Self::Create { gas_price, .. } => *gas_price,
        }
    }

    pub const fn gas_limit(&self) -> Word {
        match self {
            Self::Script { gas_limit, .. } => *gas_limit,
            Self::Create { gas_limit, .. } => *gas_limit,
        }
    }

    pub const fn maturity(&self) -> u32 {
        match self {
            Self::Script { maturity, .. } => *maturity,
            Self::Create { maturity, .. } => *maturity,
        }
    }

    pub fn inputs(&self) -> &[Input] {
        match self {
            Self::Script { inputs, .. } => inputs.as_slice(),
            Self::Create { inputs, .. } => inputs.as_slice(),
        }
    }

    pub fn inputs_mut(&mut self) -> &mut [Input] {
        match self {
            Self::Script { inputs, .. } => inputs.as_mut_slice(),
            Self::Create { inputs, .. } => inputs.as_mut_slice(),
        }
    }

    pub fn outputs(&self) -> &[Output] {
        match self {
            Self::Script { outputs, .. } => outputs.as_slice(),
            Self::Create { outputs, .. } => outputs.as_slice(),
        }
    }

    pub fn outputs_mut(&mut self) -> &mut [Output] {
        match self {
            Self::Script { outputs, .. } => outputs.as_mut_slice(),
            Self::Create { outputs, .. } => outputs.as_mut_slice(),
        }
    }

    pub fn witnesses(&self) -> &[Witness] {
        match self {
            Self::Script { witnesses, .. } => witnesses.as_slice(),
            Self::Create { witnesses, .. } => witnesses.as_slice(),
        }
    }

    pub fn witnesses_mut(&mut self) -> &mut [Witness] {
        match self {
            Self::Script { witnesses, .. } => witnesses.as_mut_slice(),
            Self::Create { witnesses, .. } => witnesses.as_mut_slice(),
        }
    }
}

const WORD_SIZE: usize = mem::size_of::<Word>();
const ID_SIZE: usize = mem::size_of::<Id>();

impl io::Read for Transaction {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Script {
                gas_price,
                gas_limit,
                maturity,
                script,
                script_data,
                inputs,
                outputs,
                witnesses,
            } => {
                let mut n = 1 + 8 * WORD_SIZE;

                if buf.len() < n {
                    return Err(bytes::eof());
                }

                buf[0] = 0x00;
                let buf = bytes::store_number_unchecked(&mut buf[1..], *gas_price);
                let buf = bytes::store_number_unchecked(buf, *gas_limit);
                let buf = bytes::store_number_unchecked(buf, *maturity);
                let buf = bytes::store_number_unchecked(buf, script.len() as Word);
                let buf = bytes::store_number_unchecked(buf, script_data.len() as Word);
                let buf = bytes::store_number_unchecked(buf, inputs.len() as Word);
                let buf = bytes::store_number_unchecked(buf, outputs.len() as Word);
                let buf = bytes::store_number_unchecked(buf, witnesses.len() as Word);

                let (size, buf) = bytes::store_raw_bytes(buf, script.as_slice())?;
                n += size;

                let (size, mut buf) = bytes::store_raw_bytes(buf, script_data.as_slice())?;
                n += size;

                for input in self.inputs_mut() {
                    let input_len = input.read(buf)?;
                    buf = &mut buf[input_len..];
                    n += input_len;
                }

                for output in self.outputs_mut() {
                    let output_len = output.read(buf)?;
                    buf = &mut buf[output_len..];
                    n += output_len;
                }

                for witness in self.witnesses_mut() {
                    let witness_len = witness.read(buf)?;
                    buf = &mut buf[witness_len..];
                    n += witness_len;
                }

                Ok(n)
            }

            Self::Create {
                gas_price,
                gas_limit,
                maturity,
                bytecode_length,
                bytecode_witness_index,
                salt,
                static_contracts,
                inputs,
                outputs,
                witnesses,
            } => {
                let mut n = 33 + 9 * WORD_SIZE + static_contracts.len() * ID_SIZE;
                if buf.len() < n {
                    return Err(bytes::eof());
                }

                buf[0] = 0x01;
                let buf = bytes::store_number_unchecked(&mut buf[1..], *gas_price);
                let buf = bytes::store_number_unchecked(buf, *gas_limit);
                let buf = bytes::store_number_unchecked(buf, *maturity);
                let buf = bytes::store_number_unchecked(buf, *bytecode_length);
                let buf = bytes::store_number_unchecked(buf, *bytecode_witness_index);
                let buf = bytes::store_number_unchecked(buf, static_contracts.len() as Word);
                let buf = bytes::store_number_unchecked(buf, inputs.len() as Word);
                let buf = bytes::store_number_unchecked(buf, outputs.len() as Word);
                let buf = bytes::store_number_unchecked(buf, witnesses.len() as Word);
                let mut buf = bytes::store_array_unchecked(buf, salt);

                for static_contract in static_contracts.iter() {
                    buf = bytes::store_array_unchecked(buf, static_contract);
                }

                for input in self.inputs_mut() {
                    let input_len = input.read(buf)?;
                    buf = &mut buf[input_len..];
                    n += input_len;
                }

                for output in self.outputs_mut() {
                    let output_len = output.read(buf)?;
                    buf = &mut buf[output_len..];
                    n += output_len;
                }

                for witness in self.witnesses_mut() {
                    let witness_len = witness.read(buf)?;
                    buf = &mut buf[witness_len..];
                    n += witness_len;
                }

                Ok(n)
            }
        }
    }
}

impl io::Write for Transaction {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The provided buffer is not big enough!",
            ));
        }

        let identifier = buf[0];
        buf = &buf[1..];

        match identifier {
            0x00 => {
                let mut n = 1 + 8 * WORD_SIZE;

                if buf.len() + 1 < n {
                    return Err(bytes::eof());
                }

                let (gas_price, buf) = bytes::restore_number_unchecked(buf);
                let (gas_limit, buf) = bytes::restore_number_unchecked(buf);
                let (maturity, buf) = bytes::restore_u32_unchecked(buf);
                let (script_len, buf) = bytes::restore_usize_unchecked(buf);
                let (script_data_len, buf) = bytes::restore_usize_unchecked(buf);
                let (inputs_len, buf) = bytes::restore_usize_unchecked(buf);
                let (outputs_len, buf) = bytes::restore_usize_unchecked(buf);
                let (witnesses_len, buf) = bytes::restore_usize_unchecked(buf);

                let (size, script, buf) = bytes::restore_raw_bytes(buf, script_len)?;
                n += size;

                let (size, script_data, mut buf) = bytes::restore_raw_bytes(buf, script_data_len)?;
                n += size;

                let mut inputs = vec![Input::contract([0x00; 32], [0x00; 32], [0x00; 32], [0x00; 32]); inputs_len];
                for input in inputs.iter_mut() {
                    let input_len = input.write(buf)?;
                    buf = &buf[input_len..];
                    n += input_len;
                }

                let mut outputs = vec![Output::contract_created([0x00; 32]); outputs_len];
                for output in outputs.iter_mut() {
                    let output_len = output.write(buf)?;
                    buf = &buf[output_len..];
                    n += output_len;
                }

                let mut witnesses = vec![Witness::default(); witnesses_len];
                for witness in witnesses.iter_mut() {
                    let witness_len = witness.write(buf)?;
                    buf = &buf[witness_len..];
                    n += witness_len;
                }

                *self = Transaction::Script {
                    gas_price,
                    gas_limit,
                    maturity,
                    script,
                    script_data,
                    inputs,
                    outputs,
                    witnesses,
                };

                Ok(n)
            }

            0x01 => {
                let mut n = 33 + 9 * WORD_SIZE;

                if buf.len() + 1 < n {
                    return Err(bytes::eof());
                }

                let (gas_price, buf) = bytes::restore_number_unchecked(buf);
                let (gas_limit, buf) = bytes::restore_number_unchecked(buf);
                let (maturity, buf) = bytes::restore_u32_unchecked(buf);
                let (bytecode_length, buf) = bytes::restore_u16_unchecked(buf);
                let (bytecode_witness_index, buf) = bytes::restore_u8_unchecked(buf);
                let (static_contracts_len, buf) = bytes::restore_usize_unchecked(buf);
                let (inputs_len, buf) = bytes::restore_usize_unchecked(buf);
                let (outputs_len, buf) = bytes::restore_usize_unchecked(buf);
                let (witnesses_len, buf) = bytes::restore_usize_unchecked(buf);
                let (salt, mut buf) = bytes::restore_array_unchecked(buf);

                if buf.len() < static_contracts_len * ID_SIZE {
                    return Err(bytes::eof());
                }

                let mut static_contracts = vec![[0x00; ID_SIZE]; static_contracts_len];
                n += ID_SIZE * static_contracts_len;
                for static_contract in static_contracts.iter_mut() {
                    static_contract.copy_from_slice(&buf[..ID_SIZE]);
                    buf = &buf[ID_SIZE..];
                }

                let mut inputs = vec![Input::contract([0x00; 32], [0x00; 32], [0x00; 32], [0x00; 32]); inputs_len];
                for input in inputs.iter_mut() {
                    let input_len = input.write(buf)?;
                    buf = &buf[input_len..];
                    n += input_len;
                }

                let mut outputs = vec![Output::contract_created([0x00; 32]); outputs_len];
                for output in outputs.iter_mut() {
                    let output_len = output.write(buf)?;
                    buf = &buf[output_len..];
                    n += output_len;
                }

                let mut witnesses = vec![Witness::default(); witnesses_len];
                for witness in witnesses.iter_mut() {
                    let witness_len = witness.write(buf)?;
                    buf = &buf[witness_len..];
                    n += witness_len;
                }

                *self = Self::Create {
                    gas_price,
                    gas_limit,
                    maturity,
                    bytecode_length,
                    bytecode_witness_index,
                    salt,
                    static_contracts,
                    inputs,
                    outputs,
                    witnesses,
                };

                Ok(n)
            }

            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The provided identifier is invalid!",
            )),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inputs_mut().iter_mut().try_for_each(|input| input.flush())?;
        self.outputs_mut().iter_mut().try_for_each(|output| output.flush())?;
        self.witnesses_mut()
            .iter_mut()
            .try_for_each(|witness| witness.flush())?;

        Ok(())
    }
}
