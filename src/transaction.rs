use crate::bytes;
use crate::consts::*;
use crate::types::Word;

use itertools::Itertools;

use std::io::Write;
use std::{io, mem};

mod types;

pub use types::{Color, Id, Input, Output, Root, ValidationError, Witness};

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
            bytecode_witness_index,
            salt,
            static_contracts,
            inputs,
            outputs,
            witnesses,
        }
    }

    pub fn validate(&self, block_height: Word) -> Result<(), ValidationError> {
        if self.gas_price() > MAX_GAS_PER_TX {
            Err(ValidationError::TransactionGasLimit)?
        }

        if block_height < self.maturity() as Word {
            Err(ValidationError::TransactionMaturity)?;
        }

        if self.inputs().len() > MAX_INPUTS {
            Err(ValidationError::TransactionInputsMax)?
        }

        if self.outputs().len() > MAX_OUTPUTS {
            Err(ValidationError::TransactionOutputsMax)?
        }

        if self.witnesses().len() > MAX_WITNESSES {
            Err(ValidationError::TransactionWitnessesMax)?
        }

        let input_colors: Vec<&Color> = self.input_colors().collect();
        for input_color in input_colors.as_slice() {
            if self
                .outputs()
                .iter()
                .filter_map(|output| match output {
                    Output::Change { color, .. } if color != &Color::default() && input_color == &color => Some(()),
                    _ => None,
                })
                .count()
                > 1
            {
                Err(ValidationError::TransactionOutputChangeColorDuplicated)?
            }
        }

        for (index, input) in self.inputs().iter().enumerate() {
            input.validate(index, self.outputs(), self.witnesses())?;
        }

        for (index, output) in self.outputs().iter().enumerate() {
            output.validate(index, self.inputs())?;
            if let Output::Change { color, .. } = output {
                if input_colors.iter().find(|input_color| input_color == &&color).is_none() {
                    Err(ValidationError::TransactionOutputChangeColorNotFound)?
                }
            }
        }

        match self {
            Self::Script {
                outputs,
                script,
                script_data,
                ..
            } => {
                if script.len() > MAX_SCRIPT_LENGTH {
                    Err(ValidationError::TransactionScriptLength)?;
                }

                if script_data.len() > MAX_SCRIPT_DATA_LENGTH {
                    Err(ValidationError::TransactionScriptDataLength)?;
                }

                outputs
                    .iter()
                    .enumerate()
                    .try_for_each(|(index, output)| match output {
                        Output::ContractCreated { .. } => {
                            Err(ValidationError::TransactionScriptOutputContractCreated { index })
                        }
                        _ => Ok(()),
                    })?;

                Ok(())
            }

            Self::Create {
                inputs,
                outputs,
                witnesses,
                bytecode_witness_index,
                static_contracts,
                ..
            } => {
                match witnesses.get(*bytecode_witness_index as usize) {
                    Some(witness) if witness.as_ref().len() as u64 * 4 > CONTRACT_MAX_SIZE => {
                        Err(ValidationError::TransactionCreateBytecodeLen)?
                    }
                    None => Err(ValidationError::TransactionCreateBytecodeWitnessIndex)?,
                    _ => (),
                }

                if static_contracts.len() > MAX_STATIC_CONTRACTS {
                    Err(ValidationError::TransactionCreateStaticContractsMax)?;
                }

                if !static_contracts.as_slice().is_sorted() {
                    Err(ValidationError::TransactionCreateStaticContractsOrder)?;
                }

                // TODO Any contract with ID in staticContracts is not in the state
                // TODO The computed contract ID (see below) is not equal to the contractID of
                // the one OutputType.ContractCreated output

                for (index, input) in inputs.iter().enumerate() {
                    match input {
                        Input::Contract { .. } => Err(ValidationError::TransactionCreateInputContract { index })?,

                        _ => (),
                    }
                }

                let mut change_color_zero = false;
                let mut contract_created = false;
                for (index, output) in outputs.iter().enumerate() {
                    match output {
                        Output::Contract { .. } => Err(ValidationError::TransactionCreateOutputContract { index })?,
                        Output::Variable { .. } => Err(ValidationError::TransactionCreateOutputVariable { index })?,

                        Output::Change { color, .. } if color == &[0u8; COLOR_SIZE] && change_color_zero => {
                            Err(ValidationError::TransactionCreateOutputChangeColorZero { index })?
                        }
                        Output::Change { color, .. } if color == &[0u8; COLOR_SIZE] => change_color_zero = true,
                        Output::Change { .. } => {
                            Err(ValidationError::TransactionCreateOutputChangeColorNonZero { index })?
                        }

                        Output::ContractCreated { .. } if contract_created => {
                            Err(ValidationError::TransactionCreateOutputContractCreatedMultiple { index })?
                        }
                        Output::ContractCreated { .. } => contract_created = true,

                        _ => (),
                    }
                }

                Ok(())
            }
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

                let bytecode_length = witnesses
                    .get(*bytecode_witness_index as usize)
                    .map(|witness| witness.as_ref().len() as Word / 4)
                    .unwrap_or(0);

                buf[0] = 0x01;
                let buf = bytes::store_number_unchecked(&mut buf[1..], *gas_price);
                let buf = bytes::store_number_unchecked(buf, *gas_limit);
                let buf = bytes::store_number_unchecked(buf, *maturity);
                let buf = bytes::store_number_unchecked(buf, bytecode_length);
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
                let (_bytecode_length, buf) = bytes::restore_u16_unchecked(buf);
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
