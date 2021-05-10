use super::{Color, Id, Root};
use crate::bytes;
use crate::types::Word;

use std::{io, mem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Output {
    Coin {
        to: Id,
        amount: Word,
        color: Color,
    } = 0x00,

    Contract {
        input_index: u8,
        balance_root: Root,
        state_root: Root,
    } = 0x01,

    Withdrawal {
        to: Id,
        amount: Word,
        color: Color,
    } = 0x02,

    Change {
        to: Id,
        amount: Word,
        color: Color,
    } = 0x03,

    Variable {
        to: Id,
        amount: Word,
        color: Color,
    } = 0x04,

    ContractCreated {
        contract_id: Id,
    } = 0x05,
}

impl Output {
    pub const fn coin(to: Id, amount: Word, color: Color) -> Self {
        Self::Coin { to, amount, color }
    }

    pub const fn contract(input_index: u8, balance_root: Root, state_root: Root) -> Self {
        Self::Contract {
            input_index,
            balance_root,
            state_root,
        }
    }

    pub const fn withdrawal(to: Id, amount: Word, color: Color) -> Self {
        Self::Withdrawal { to, amount, color }
    }

    pub const fn change(to: Id, amount: Word, color: Color) -> Self {
        Self::Change { to, amount, color }
    }

    pub const fn variable(to: Id, amount: Word, color: Color) -> Self {
        Self::Variable { to, amount, color }
    }

    pub const fn contract_created(contract_id: Id) -> Self {
        Self::ContractCreated { contract_id }
    }
}

const ID_SIZE: usize = mem::size_of::<Id>();
const WORD_SIZE: usize = mem::size_of::<Word>();
const COLOR_SIZE: usize = mem::size_of::<Color>();
const ROOT_SIZE: usize = mem::size_of::<Root>();

impl io::Read for Output {
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
        let identifier = match self {
            Self::Coin { .. } => 0x00,
            Self::Contract { .. } => 0x01,
            Self::Withdrawal { .. } => 0x02,
            Self::Change { .. } => 0x03,
            Self::Variable { .. } => 0x04,
            Self::ContractCreated { .. } => 0x05,
        };

        match self {
            Self::Coin { .. } | Self::Withdrawal { .. } | Self::Change { .. } | Self::Variable { .. }
                if buf.len() < 1 + ID_SIZE + WORD_SIZE + COLOR_SIZE =>
            {
                Err(bytes::eof())
            }

            Self::Contract { .. } if buf.len() < 1 + WORD_SIZE + 2 * ROOT_SIZE => Err(bytes::eof()),

            Self::ContractCreated { .. } if buf.len() < 1 + ID_SIZE => Err(bytes::eof()),

            Self::Coin { to, amount, color }
            | Self::Withdrawal { to, amount, color }
            | Self::Change { to, amount, color }
            | Self::Variable { to, amount, color } => {
                buf[0] = identifier;
                buf = bytes::store_array_unchecked(&mut buf[1..], to);
                buf = bytes::store_number_unchecked(buf, *amount);
                bytes::store_array_unchecked(buf, color);

                Ok(1 + ID_SIZE + WORD_SIZE + COLOR_SIZE)
            }

            Self::Contract {
                input_index,
                balance_root,
                state_root,
            } => {
                buf[0] = identifier;
                buf = bytes::store_number_unchecked(&mut buf[1..], *input_index);
                buf = bytes::store_array_unchecked(buf, balance_root);
                bytes::store_array_unchecked(buf, state_root);

                Ok(1 + WORD_SIZE + 2 * ROOT_SIZE)
            }

            Self::ContractCreated { contract_id } => {
                buf[0] = identifier;
                bytes::store_array_unchecked(&mut buf[1..], contract_id);

                Ok(1 + ID_SIZE)
            }
        }
    }
}

impl io::Write for Output {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Err(bytes::eof());
        }

        let identifier = buf[0];
        buf = &buf[1..];

        match identifier {
            0x00 | 0x02 | 0x03 | 0x4 if buf.len() < ID_SIZE + WORD_SIZE + COLOR_SIZE => Err(bytes::eof()),

            0x01 if buf.len() < WORD_SIZE + 2 * ROOT_SIZE => Err(bytes::eof()),

            0x05 if buf.len() < ID_SIZE => Err(bytes::eof()),

            0x00 | 0x02 | 0x03 | 0x4 => {
                let (to, buf) = bytes::restore_array_unchecked(buf);
                let (amount, buf) = bytes::restore_number_unchecked(buf);
                let (color, _) = bytes::restore_array_unchecked(buf);

                match identifier {
                    0x00 => *self = Self::Coin { to, amount, color },
                    0x02 => *self = Self::Withdrawal { to, amount, color },
                    0x03 => *self = Self::Change { to, amount, color },
                    0x04 => *self = Self::Variable { to, amount, color },

                    _ => unreachable!(),
                }

                Ok(1 + ID_SIZE + WORD_SIZE + COLOR_SIZE)
            }

            0x01 => {
                let (input_index, buf) = bytes::restore_u8_unchecked(buf);
                let (balance_root, buf) = bytes::restore_array_unchecked(buf);
                let (state_root, _) = bytes::restore_array_unchecked(buf);

                *self = Self::Contract {
                    input_index,
                    balance_root,
                    state_root,
                };

                Ok(1 + WORD_SIZE + 2 * ROOT_SIZE)
            }

            0x05 => {
                let (contract_id, _) = bytes::restore_array_unchecked(buf);
                *self = Self::ContractCreated { contract_id };

                Ok(1 + ID_SIZE)
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
