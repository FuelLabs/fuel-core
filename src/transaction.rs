use crate::types::Word;

use std::convert::TryFrom;
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
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
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
                let script_len = script.len();
                let script_data_len = script_data.len();

                let mut n = 45 + 2 * WORD_SIZE + script_len + script_data_len;

                if buf.len() < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                buf[0] = 0x00;
                buf = &mut buf[1..];

                buf[..WORD_SIZE].copy_from_slice(&gas_price.to_be_bytes()[..]);
                buf = &mut buf[WORD_SIZE..];

                buf[..WORD_SIZE].copy_from_slice(&gas_limit.to_be_bytes()[..]);
                buf = &mut buf[WORD_SIZE..];

                buf[..4].copy_from_slice(&maturity.to_be_bytes()[..]);
                buf = &mut buf[4..];

                buf[..8].copy_from_slice(&(script_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                buf[..8].copy_from_slice(&(script_data_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                let inputs_len = inputs.len();
                buf[..8].copy_from_slice(&(inputs_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                let outputs_len = outputs.len();
                buf[..8].copy_from_slice(&(outputs_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                let witnesses_len = witnesses.len();
                buf[..8].copy_from_slice(&(witnesses_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                buf[..script_len].copy_from_slice(script.as_slice());
                buf = &mut buf[script_len..];

                buf[..script_data_len].copy_from_slice(script_data.as_slice());
                buf = &mut buf[script_data_len..];

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
                let static_contracts_len = static_contracts.len();
                let inputs_len = inputs.len();
                let outputs_len = outputs.len();
                let witnesses_len = witnesses.len();

                let mut n = 72 + 2 * WORD_SIZE + static_contracts_len * ID_SIZE;

                if buf.len() < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                buf[0] = 0x01;
                buf = &mut buf[1..];

                buf[..WORD_SIZE].copy_from_slice(&gas_price.to_be_bytes()[..]);
                buf = &mut buf[WORD_SIZE..];

                buf[..WORD_SIZE].copy_from_slice(&gas_limit.to_be_bytes()[..]);
                buf = &mut buf[WORD_SIZE..];

                buf[..4].copy_from_slice(&maturity.to_be_bytes()[..]);
                buf = &mut buf[4..];

                buf[..2].copy_from_slice(&bytecode_length.to_be_bytes()[..]);
                buf = &mut buf[2..];

                buf[0] = *bytecode_witness_index;
                buf = &mut buf[1..];

                buf[..32].copy_from_slice(&salt[..]);
                buf = &mut buf[32..];

                buf[..8].copy_from_slice(&(static_contracts_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                buf[..8].copy_from_slice(&(inputs_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                buf[..8].copy_from_slice(&(outputs_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                buf[..8].copy_from_slice(&(witnesses_len as u64).to_be_bytes()[..]);
                buf = &mut buf[8..];

                for static_contract in static_contracts.iter() {
                    buf[..ID_SIZE].copy_from_slice(static_contract);
                    buf = &mut buf[ID_SIZE..];
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
                let mut n = 45 + 2 * WORD_SIZE;

                if buf.len() + 1 < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let gas_price = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[WORD_SIZE..];
                let gas_price = Word::from_be_bytes(gas_price);

                let gas_limit = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[WORD_SIZE..];
                let gas_limit = Word::from_be_bytes(gas_limit);

                let maturity = <[u8; 4]>::try_from(&buf[..4]).unwrap_or_else(|_| unreachable!());
                buf = &buf[4..];
                let maturity = u32::from_be_bytes(maturity);

                let script_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let script_len = u64::from_be_bytes(script_len) as usize;

                let script_data_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let script_data_len = u64::from_be_bytes(script_data_len) as usize;

                let inputs_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let inputs_len = u64::from_be_bytes(inputs_len) as usize;

                let outputs_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let outputs_len = u64::from_be_bytes(outputs_len) as usize;

                let witnesses_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let witnesses_len = u64::from_be_bytes(witnesses_len) as usize;

                if buf.len() < script_len + script_data_len {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let script = (&buf[..script_len]).to_vec();
                buf = &buf[script_len..];
                n += script_len;

                let script_data = (&buf[..script_data_len]).to_vec();
                buf = &buf[script_data_len..];
                n += script_data_len;

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
                let mut n = 72 + 2 * WORD_SIZE;

                if buf.len() + 1 < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let gas_price = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[WORD_SIZE..];
                let gas_price = Word::from_be_bytes(gas_price);

                let gas_limit = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[WORD_SIZE..];
                let gas_limit = Word::from_be_bytes(gas_limit);

                let maturity = <[u8; 4]>::try_from(&buf[..4]).unwrap_or_else(|_| unreachable!());
                buf = &buf[4..];
                let maturity = u32::from_be_bytes(maturity);

                let bytecode_length = <[u8; 2]>::try_from(&buf[..2]).unwrap_or_else(|_| unreachable!());
                buf = &buf[2..];
                let bytecode_length = u16::from_be_bytes(bytecode_length);

                let bytecode_witness_index = buf[0];
                buf = &buf[1..];

                let salt = <[u8; 32]>::try_from(&buf[..32]).unwrap_or_else(|_| unreachable!());
                buf = &buf[32..];

                let static_contracts_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let static_contracts_len = u64::from_be_bytes(static_contracts_len) as usize;

                let inputs_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let inputs_len = u64::from_be_bytes(inputs_len) as usize;

                let outputs_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let outputs_len = u64::from_be_bytes(outputs_len) as usize;

                let witnesses_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let witnesses_len = u64::from_be_bytes(witnesses_len) as usize;

                if buf.len() < static_contracts_len * ID_SIZE {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
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
