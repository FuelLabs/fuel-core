use bincode::config::{Configuration, LittleEndian, NoLimit, Varint};
use std::{
    io::{BufRead, BufReader, BufWriter, Cursor, IntoInnerError, Read, Seek, Write},
    marker::PhantomData,
    path::Path,
    sync::{atomic::AtomicU64, Arc},
};

use fuel_core_types::fuel_types::{Bytes32, ContractId};

use crate::{CoinConfig, ContractConfig, MessageConfig};

#[derive(Clone, Debug)]
pub struct StateImporter<T> {
    phantom_data: PhantomData<T>,
}

pub enum ContractComponent {
    ContractMetadata(ContractConfig),
    ContractState(ContractId, Bytes32, Bytes32),
    ContractAsset(ContractId, Bytes32, u64),
}

impl<T> StateImporter<T> {
    pub fn next(&mut self) -> Option<anyhow::Result<T>> {
        todo!()
    }

    pub fn current_cursor(&self) -> usize {
        todo!()
    }
}

impl StateImporter<CoinConfig> {
    pub fn new(_reader: impl std::io::Read) -> Self {
        Self {
            phantom_data: PhantomData::default(),
        }
    }

    pub fn local_testnet() -> Self {
        Self {
            phantom_data: PhantomData::default(),
        }
    }

    pub fn load_from_file(_path: impl AsRef<Path>) -> Self {
        Self {
            phantom_data: PhantomData::default(),
        }
    }

    pub fn messages(self) -> StateImporter<MessageConfig> {
        StateImporter::<MessageConfig> {
            phantom_data: PhantomData,
        }
    }
}

impl StateImporter<MessageConfig> {
    pub fn contracts(self) -> StateImporter<ContractComponent> {
        StateImporter::<ContractComponent> {
            phantom_data: PhantomData,
        }
    }
}

impl<T> Iterator for StateImporter<T> {
    type Item = anyhow::Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next()
    }
}
