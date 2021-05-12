use crate::bytes;
use crate::consts::*;
use crate::interpreter::Interpreter;
use crate::types::Word;

use itertools::Itertools;

use std::io::Write;
use std::{io, mem};

mod types;

pub use types::{Color, Id, Input, Output, Root, Witness};

const COLOR_SIZE: usize = mem::size_of::<Color>();

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

    pub fn is_valid(&self, vm: &Interpreter) -> bool {
        // TODO test and optimize
        self.gas_limit() <= MAX_GAS_PER_TX
            && self.inputs().len() <= MAX_INPUTS
            && self.outputs().len() <= MAX_OUTPUTS
            && self.witnesses().len() <= MAX_WITNESSES
            && vm.block_height() >= self.maturity()

            // Grant every unique input color contains zero or one corresponding output change
            // color
            && 1 == self.input_colors().fold(1, |acc, input_color| {
                acc * 1.max(
                    self.outputs()
                        .iter()
                        .filter_map(|output| match output {
                            Output::Change { color, .. } if color == input_color => Some(()),
                            _ => None,
                        })
                        .count(),
                )
            })

            // TODO use the interpreter to get the unique colors list from stack
            // Grant every output change color exists in the inputs color
            && 1 == self.outputs()
                .iter()
                .filter_map(|output| match output {
                    Output::Change { color, .. } => Some(color),
                    _ => None,
                })
                .dedup()
                .fold(1, |acc, output_color| {
                    acc * self
                        .input_colors()
                        .find(|input_color| input_color == &output_color)
                        .map(|_| 1)
                        .unwrap_or(0)
                })

                && self.is_valid_script()
                && self.is_valid_create(vm)

                && self.inputs().iter().fold(true, |acc, input| acc && input.is_valid())
                && self.outputs().iter().fold(true, |acc, output| acc && output.is_valid())
    }

    fn is_valid_script(&self) -> bool {
        match self {
            Self::Script {
                script, script_data, ..
            } => {
                // Invalid: any output is of type OutputType.ContractCreated
                self.outputs()
                    .iter()
                    .filter_map(|output| match output {
                        Output::ContractCreated { .. } => Some(()),
                        _ => None,
                    })
                    .count()
                    == 0
                    && script.len() <= MAX_SCRIPT_LENGTH
                    && script_data.len() <= MAX_SCRIPT_DATA_LENGTH
            }
            _ => true,
        }
    }

    fn is_valid_create(&self, _vm: &Interpreter) -> bool {
        match self {
            Self::Create {
                bytecode_length,
                bytecode_witness_index,
                static_contracts,
                witnesses,
                ..
            } => {
                (*bytecode_length as u64) * 4 <= CONTRACT_MAX_SIZE

                    && static_contracts.len() <= MAX_STATIC_CONTRACTS
                    && static_contracts.as_slice().is_sorted()
                    // TODO Any contract with ID in staticContracts is not in the state
                    // TODO The computed contract ID (see below) is not equal to the contractID
                    // of the one OutputType.ContractCreated output

                    // tx.data.witnesses[bytecodeWitnessIndex].dataLength != bytecodeLength * 4
                    && witnesses.get(*bytecode_witness_index as usize)
                        .map(|witness| witness.as_ref().len() == (*bytecode_length as usize) * 4)
                        .unwrap_or(false)

                    // Invalid: Any input is of type InputType.Contract
                    && 0 == self
                        .inputs()
                        .iter()
                        .filter_map(|input| match input {
                            Input::Contract { .. } => Some(()),
                            _ => None,
                        })
                        .count()

                    // Invalid: Any output is of type OutputType.Contract or OutputType.Variable
                    && 0 == self
                        .outputs()
                        .iter()
                        .filter_map(|output| match output {
                            Output::Contract { .. } | Output::Variable { .. } => Some(()),
                            _ => None,
                        })
                        .count()

                    // Invalid: More than one output is of type OutputType.Change with color of zero
                    && 1 <= self
                        .outputs()
                        .iter()
                        .filter_map(|output| match output.color() {
                            Some(c) if c == &[0; COLOR_SIZE] => Some(()),
                            _ => None,
                        })
                        .count()

                    // Invalid: Any output is of type OutputType.Change with non-zero color
                    && 0 == self.outputs().iter().filter_map(|output| match output {
                        Output::Change { color, .. } if color != &[0; COLOR_SIZE] => Some(()),
                        _ => None,
                    }).count()

                    // Invalid: More than one output is of type OutputType.ContractCreated
                    && 1 <= self
                        .outputs()
                        .iter()
                        .filter_map(|output| match output {
                            Output::ContractCreated { .. } => Some(()),
                            _ => None,
                        })
                        .count()
            }

            _ => true,
        }
    }

    pub fn input_colors(&self) -> impl Iterator<Item = &Color> {
        self.inputs()
            .iter()
            .filter_map(|input| match input {
                Input::Coin { color, .. } => Some(color),
                _ => None,
            })
            .dedup()
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

    pub const fn is_script(&self) -> bool {
        match self {
            Self::Script { .. } => true,
            _ => false,
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

    pub fn try_from_bytes(bytes: &[u8]) -> io::Result<(usize, Self)> {
        let mut tx = Self::script(0, 0, 0, vec![], vec![], vec![], vec![], vec![]);

        let n = tx.write(bytes)?;

        Ok((n, tx))
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
