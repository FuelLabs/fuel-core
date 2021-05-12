use super::{Color, Id, Root};
use crate::bytes;
use crate::consts::*;
use crate::types::Word;

use std::{io, mem};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Input {
    Coin {
        utxo_id: Id,
        owner: Id,
        amount: Word,
        color: Color,
        witness_index: u8,
        maturity: u32,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    } = 0x00,

    Contract {
        utxo_id: Id,
        balance_root: Root,
        state_root: Root,
        contract_id: Id,
    } = 0x01,
}

impl Input {
    pub const fn coin(
        utxo_id: Id,
        owner: Id,
        amount: Word,
        color: Color,
        witness_index: u8,
        maturity: u32,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    ) -> Self {
        Self::Coin {
            utxo_id,
            owner,
            amount,
            color,
            witness_index,
            maturity,
            predicate,
            predicate_data,
        }
    }

    pub const fn contract(utxo_id: Id, balance_root: Root, state_root: Root, contract_id: Id) -> Self {
        Self::Contract {
            utxo_id,
            balance_root,
            state_root,
            contract_id,
        }
    }

    pub const fn utxo_id(&self) -> &Id {
        match self {
            Self::Coin { utxo_id, .. } => &utxo_id,
            Self::Contract { utxo_id, .. } => &utxo_id,
        }
    }

    pub fn is_valid(&self) -> bool {
        match self {
            // TODO If h is the block height the UTXO being spent was created,
            // transaction is invalid if blockheight() < h + maturity.
            Self::Coin {
                predicate,
                predicate_data,
                ..
            } => predicate.len() <= MAX_PREDICATE_LENGTH && predicate_data.len() <= MAX_PREDICATE_DATA_LENGTH,

            // TODO there is not exactly one output of type OutputType.Contract
            // with inputIndex equal to this input's index
            Self::Contract { .. } => true,
        }
    }
}

const ID_SIZE: usize = mem::size_of::<Id>();
const WORD_SIZE: usize = mem::size_of::<Word>();
const COLOR_SIZE: usize = mem::size_of::<Color>();
const ROOT_SIZE: usize = mem::size_of::<Root>();

impl io::Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Coin {
                utxo_id,
                owner,
                amount,
                color,
                witness_index,
                maturity,
                predicate,
                predicate_data,
            } => {
                let mut n = 1 + 2 * ID_SIZE + 5 * WORD_SIZE + COLOR_SIZE;

                if buf.len() < n {
                    return Err(bytes::eof());
                }

                buf[0] = 0x00;
                let buf = bytes::store_array_unchecked(&mut buf[1..], utxo_id);
                let buf = bytes::store_array_unchecked(buf, owner);
                let buf = bytes::store_number_unchecked(buf, *amount);
                let buf = bytes::store_array_unchecked(buf, color);
                let buf = bytes::store_number_unchecked(buf, *witness_index);
                let buf = bytes::store_number_unchecked(buf, *maturity);

                let buf = bytes::store_number_unchecked(buf, predicate.len() as Word);
                let buf = bytes::store_number_unchecked(buf, predicate_data.len() as Word);

                let (size, buf) = bytes::store_raw_bytes(buf, predicate.as_slice())?;
                n += size;

                let (size, _) = bytes::store_raw_bytes(buf, predicate_data.as_slice())?;
                n += size;

                Ok(n)
            }

            Self::Contract { .. } if buf.len() < 1 + 2 * ID_SIZE + 2 * ROOT_SIZE => Err(bytes::eof()),

            Self::Contract {
                utxo_id,
                balance_root,
                state_root,
                contract_id,
            } => {
                buf[0] = 0x01;
                let buf = bytes::store_array_unchecked(&mut buf[1..], utxo_id);
                let buf = bytes::store_array_unchecked(buf, balance_root);
                let buf = bytes::store_array_unchecked(buf, state_root);
                bytes::store_array_unchecked(buf, contract_id);

                Ok(1 + 2 * ID_SIZE + 2 * ROOT_SIZE)
            }
        }
    }
}

impl io::Write for Input {
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
            0x00 if buf.len() < 2 * ID_SIZE + 5 * WORD_SIZE + COLOR_SIZE => Err(bytes::eof()),

            0x00 => {
                let mut n = 1 + 2 * ID_SIZE + 5 * WORD_SIZE + COLOR_SIZE;

                let (utxo_id, buf) = bytes::restore_array_unchecked(buf);
                let (owner, buf) = bytes::restore_array_unchecked(buf);
                let (amount, buf) = bytes::restore_number_unchecked(buf);
                let (color, buf) = bytes::restore_array_unchecked(buf);
                let (witness_index, buf) = bytes::restore_u8_unchecked(buf);
                let (maturity, buf) = bytes::restore_u32_unchecked(buf);

                let (predicate_len, buf) = bytes::restore_usize_unchecked(buf);
                let (predicate_data_len, buf) = bytes::restore_usize_unchecked(buf);

                let (size, predicate, buf) = bytes::restore_raw_bytes(buf, predicate_len)?;
                n += size;

                let (size, predicate_data, _) = bytes::restore_raw_bytes(buf, predicate_data_len)?;
                n += size;

                *self = Self::Coin {
                    utxo_id,
                    owner,
                    amount,
                    color,
                    witness_index,
                    maturity,
                    predicate,
                    predicate_data,
                };

                Ok(n)
            }

            0x01 if buf.len() < 2 * ID_SIZE + 2 * ROOT_SIZE => Err(bytes::eof()),

            0x01 => {
                let (utxo_id, buf) = bytes::restore_array_unchecked(buf);
                let (balance_root, buf) = bytes::restore_array_unchecked(buf);
                let (state_root, buf) = bytes::restore_array_unchecked(buf);
                let (contract_id, _) = bytes::restore_array_unchecked(buf);

                *self = Self::Contract {
                    utxo_id,
                    balance_root,
                    state_root,
                    contract_id,
                };

                Ok(1 + 2 * ROOT_SIZE + 2 * ID_SIZE)
            }

            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The provided identifier is invalid!",
            )),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
