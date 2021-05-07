use super::{Color, Id, Root};
use crate::types::Word;

use std::convert::TryFrom;
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
}

impl io::Read for Input {
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
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
                let amount = amount.to_be_bytes();
                let maturity = maturity.to_be_bytes();

                let utxo_id_len = utxo_id.len();
                let owner_len = owner.len();
                let amount_len = amount.len();
                let color_len = color.len();
                let maturity_len = maturity.len();
                let predicate_len = predicate.len();
                let predicate_data_len = predicate_data.len();

                let n = 18
                    + utxo_id_len
                    + owner_len
                    + amount_len
                    + color_len
                    + maturity_len
                    + predicate_len
                    + predicate_data_len;

                if buf.len() < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                buf[0] = 0x00;
                buf = &mut buf[1..];

                buf[..utxo_id_len].copy_from_slice(&utxo_id[..]);
                buf = &mut buf[utxo_id_len..];

                buf[..owner_len].copy_from_slice(&owner[..]);
                buf = &mut buf[owner_len..];

                buf[..amount_len].copy_from_slice(&amount[..]);
                buf = &mut buf[amount_len..];

                buf[..color_len].copy_from_slice(&color[..]);
                buf = &mut buf[color_len..];

                buf[0] = *witness_index;
                buf = &mut buf[1..];

                buf[..maturity_len].copy_from_slice(&maturity[..]);
                buf = &mut buf[maturity_len..];

                let predicate_len_bytes = (predicate_len as u64).to_be_bytes();
                buf[..8].copy_from_slice(&predicate_len_bytes[..]);
                buf = &mut buf[8..];

                buf[..predicate_len].copy_from_slice(predicate.as_slice());
                buf = &mut buf[predicate_len..];

                let predicate_data_len_bytes = (predicate_data_len as u64).to_be_bytes();
                buf[..8].copy_from_slice(&predicate_data_len_bytes[..]);
                buf = &mut buf[8..];

                buf[..predicate_data_len].copy_from_slice(predicate_data.as_slice());

                Ok(n)
            }

            Self::Contract {
                utxo_id,
                balance_root,
                state_root,
                contract_id,
            } => {
                let utxo_id_len = utxo_id.len();
                let balance_root_len = balance_root.len();
                let state_root_len = state_root.len();
                let contract_id_len = contract_id.len();

                let n = 1 + utxo_id_len + balance_root_len + state_root_len + contract_id_len;

                if buf.len() < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                buf[0] = 0x01;
                buf = &mut buf[1..];

                buf[..utxo_id_len].copy_from_slice(&utxo_id[..]);
                buf = &mut buf[utxo_id_len..];

                buf[..balance_root_len].copy_from_slice(&balance_root[..]);
                buf = &mut buf[balance_root_len..];

                buf[..state_root_len].copy_from_slice(&state_root[..]);
                buf = &mut buf[state_root_len..];

                buf[..contract_id_len].copy_from_slice(&contract_id[..]);

                Ok(n)
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

        const ID_SIZE: usize = mem::size_of::<Id>();
        const WORD_SIZE: usize = mem::size_of::<Word>();
        const COLOR_SIZE: usize = mem::size_of::<Color>();
        const ROOT_SIZE: usize = mem::size_of::<Root>();

        match identifier {
            0x00 => {
                let mut n = 22 + 2 * ID_SIZE + WORD_SIZE + COLOR_SIZE;

                if buf.len() + 1 < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let utxo_id = Id::try_from(&buf[..ID_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[ID_SIZE..];

                let owner = Id::try_from(&buf[..ID_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[ID_SIZE..];

                let amount = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[WORD_SIZE..];
                let amount = Word::from_be_bytes(amount);

                let color = Color::try_from(&buf[..COLOR_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[COLOR_SIZE..];

                let witness_index = buf[0];
                buf = &buf[1..];

                let maturity = <[u8; 4]>::try_from(&buf[..4]).unwrap_or_else(|_| unreachable!());
                buf = &buf[4..];
                let maturity = u32::from_be_bytes(maturity);

                let predicate_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let predicate_len = u64::from_be_bytes(predicate_len) as usize;

                if buf.len() < predicate_len {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let predicate = (&buf[..predicate_len]).to_vec();
                buf = &buf[predicate_len..];
                n += predicate_len;

                if buf.len() < 8 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let predicate_data_len = <[u8; 8]>::try_from(&buf[..8]).unwrap_or_else(|_| unreachable!());
                buf = &buf[8..];
                let predicate_data_len = u64::from_be_bytes(predicate_data_len) as usize;

                if buf.len() < predicate_data_len {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let predicate_data = (&buf[..predicate_data_len]).to_vec();
                n += predicate_data_len;

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

            0x01 => {
                let n = 1 + 2 * (ROOT_SIZE + ID_SIZE);

                if buf.len() + 1 < n {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "The provided buffer is not big enough!",
                    ));
                }

                let utxo_id = Id::try_from(&buf[..ID_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[ID_SIZE..];

                let balance_root = Root::try_from(&buf[..ROOT_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[ROOT_SIZE..];

                let state_root = Root::try_from(&buf[..ROOT_SIZE]).unwrap_or_else(|_| unreachable!());
                buf = &buf[ROOT_SIZE..];

                let contract_id = Id::try_from(&buf[..ID_SIZE]).unwrap_or_else(|_| unreachable!());

                *self = Self::Contract {
                    utxo_id,
                    balance_root,
                    state_root,
                    contract_id,
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
        Ok(())
    }
}
