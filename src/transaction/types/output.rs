use super::{Color, Id, Root};
use crate::types::Word;

use std::convert::TryFrom;
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
            Self::Coin { to, amount, color }
            | Self::Withdrawal { to, amount, color }
            | Self::Change { to, amount, color }
            | Self::Variable { to, amount, color } => {
                let n = 1 + ID_SIZE + WORD_SIZE + COLOR_SIZE;

                if buf.len() < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                buf[0] = identifier;
                buf = &mut buf[1..];

                buf[..ID_SIZE].copy_from_slice(&to[..]);
                buf = &mut buf[ID_SIZE..];

                buf[..WORD_SIZE].copy_from_slice(&amount.to_be_bytes()[..]);
                buf = &mut buf[WORD_SIZE..];

                buf[..COLOR_SIZE].copy_from_slice(&color[..]);

                Ok(n)
            }

            Self::Contract {
                input_index,
                balance_root,
                state_root,
            } => {
                let n = 2 + 2 * ROOT_SIZE;

                if buf.len() < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                buf[0] = identifier;
                buf[1] = *input_index;
                buf = &mut buf[2..];

                buf[..ROOT_SIZE].copy_from_slice(&balance_root[..]);
                buf = &mut buf[ROOT_SIZE..];

                buf[..ROOT_SIZE].copy_from_slice(&state_root[..]);

                Ok(n)
            }

            Self::ContractCreated { contract_id } => {
                let n = 1 + ID_SIZE;

                if buf.len() < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                buf[0] = identifier;
                buf[1..1 + ID_SIZE].copy_from_slice(&contract_id[..]);

                Ok(n)
            }
        }
    }
}

impl io::Write for Output {
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
            0x00 | 0x02 | 0x03 | 0x4 => {
                let n = 1 + ID_SIZE + WORD_SIZE + COLOR_SIZE;

                if buf.len() + 1 < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let to = Id::try_from(&buf[..ID_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[ID_SIZE..];

                let amount = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[WORD_SIZE..];
                let amount = Word::from_be_bytes(amount);

                let color = Id::try_from(&buf[..COLOR_SIZE]).unwrap_or_else(|_| unreachable!());

                match identifier {
                    0x00 => *self = Self::Coin { to, amount, color },
                    0x02 => *self = Self::Withdrawal { to, amount, color },
                    0x03 => *self = Self::Change { to, amount, color },
                    0x04 => *self = Self::Variable { to, amount, color },

                    _ => unreachable!(),
                }

                Ok(n)
            }

            0x01 => {
                let n = 2 + 2 * ROOT_SIZE;

                if buf.len() + 1 < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let input_index = buf[0];

                let balance_root = Root::try_from(&buf[1..1 + ROOT_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[1 + ID_SIZE..];

                let state_root = Root::try_from(&buf[..ROOT_SIZE]).unwrap_or_else(|_| unreachable!());

                *self = Self::Contract {
                    input_index,
                    balance_root,
                    state_root,
                };

                Ok(n)
            }

            0x05 => {
                let n = 1 + ID_SIZE;

                if buf.len() + 1 < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let contract_id = Id::try_from(&buf[..ID_SIZE]).unwrap_or_else(|_| unreachable!());
                *self = Self::ContractCreated { contract_id };

                Ok(n)
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
