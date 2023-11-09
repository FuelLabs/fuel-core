use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::{Address, AssetId, BlockHeight, Bytes32, ContractId, Nonce},
    fuel_vm::Salt,
};
use parquet::{
    file::{reader::ChunkReader, serialized_reader::SerializedFileReader},
    record::{Field, Row},
};

use crate::{
    config::{
        codec::{Batch, BatchReader},
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig,
};

use super::schema::ParquetSchema;

pub(crate) struct ParquetBatchReader<R: ChunkReader> {
    batch_size: usize,
    compression_level: u32,
    reader: SerializedFileReader<R>,
}

impl<R: ChunkReader + 'static> ParquetBatchReader<R> {
    pub fn new(
        batch_size: usize,
        compression_level: u32,
        reader: R,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            batch_size,
            compression_level,
            reader: SerializedFileReader::new(reader)?,
        })
    }
}

impl<T, R> BatchReader<T> for ParquetBatchReader<R>
where
    T: ParquetSchema + From<Row>,
    R: ChunkReader + 'static,
{
    fn read_batch(&mut self) -> anyhow::Result<Option<Batch<T>>> {
        todo!()
        // self.reader.get_row_iter(projection)
        // for row in reader.get_row_iter(Some(T::schema())).unwrap() {
        //     let _ = T::from(row.unwrap());
        // }
    }
}

impl From<Row> for CoinConfig {
    fn from(row: Row) -> Self {
        let mut iter = row.get_column_iter();

        let tx_id = match iter.next().unwrap().1 {
            Field::Null => None,
            Field::Bytes(tx_id) => Some(tx_id),
            _ => panic!("Unexpected type!"),
        };
        let tx_id = tx_id.map(|bytes| Bytes32::new(bytes.data().try_into().unwrap()));

        let output_index = match iter.next().unwrap().1 {
            Field::UByte(output_index) => Some(*output_index),
            Field::Null => None,
            _ => panic!("Should not happen"),
        };

        let tx_pointer_block_height = match iter.next().unwrap().1 {
            Field::UInt(tx_pointer_block_height) => Some(*tx_pointer_block_height),
            Field::Null => None,
            _ => panic!("Should not happen"),
        };
        let tx_pointer_block_height = tx_pointer_block_height.map(BlockHeight::new);

        let tx_pointer_tx_idx = match iter.next().unwrap().1 {
            Field::UShort(tx_pointer_tx_idx) => Some(*tx_pointer_tx_idx),
            Field::Null => None,
            _ => panic!("Should not happen"),
        };
        let maturity = match iter.next().unwrap().1 {
            Field::UInt(maturity) => Some(*maturity),
            Field::Null => None,
            _ => panic!("Should not happen"),
        };
        let maturity = maturity.map(BlockHeight::new);

        let Field::Bytes(owner) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let owner = Address::new(owner.data().try_into().unwrap());

        let Field::ULong(amount) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let amount = *amount;

        let Field::Bytes(asset_id) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let asset_id = AssetId::new(asset_id.data().try_into().unwrap());

        Self {
            tx_id,
            output_index,
            tx_pointer_block_height,
            tx_pointer_tx_idx,
            maturity,
            owner,
            amount,
            asset_id,
        }
    }
}

impl From<Row> for MessageConfig {
    fn from(row: Row) -> Self {
        let mut iter = row.get_column_iter();

        let Field::Bytes(sender) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let sender = Address::new(sender.data().try_into().unwrap());

        let Field::Bytes(recipient) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let recipient = Address::new(recipient.data().try_into().unwrap());

        let Field::Bytes(nonce) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let nonce = Nonce::new(nonce.data().try_into().unwrap());

        let Field::ULong(amount) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let amount = *amount;

        let Field::Bytes(data) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let data = data.data().to_vec();

        let Field::ULong(da_height) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let da_height = DaBlockHeight(*da_height);

        Self {
            sender,
            recipient,
            nonce,
            amount,
            data,
            da_height,
        }
    }
}

impl From<Row> for ContractState {
    fn from(row: Row) -> Self {
        let mut iter = row.get_column_iter();

        let Field::Bytes(key) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let key = Bytes32::new(key.data().try_into().unwrap());
        let Field::Bytes(value) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let value = Bytes32::new(value.data().try_into().unwrap());

        Self { key, value }
    }
}

impl From<Row> for ContractBalance {
    fn from(row: Row) -> Self {
        let mut iter = row.get_column_iter();

        let Field::Bytes(asset_id) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let asset_id = AssetId::new(asset_id.data().try_into().unwrap());

        let Field::ULong(amount) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let amount = *amount;

        Self { asset_id, amount }
    }
}

impl From<Row> for ContractConfig {
    fn from(row: Row) -> Self {
        let mut iter = row.get_column_iter();

        let (_, Field::Bytes(contract_id)) = iter.next().unwrap() else {
            panic!("Unexpected type!");
        };
        let contract_id = ContractId::new(contract_id.data().try_into().unwrap());

        let Field::Bytes(code) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let code = Vec::from(code.data());

        let Field::Bytes(salt) = iter.next().unwrap().1 else {
            panic!("Unexpected type!");
        };
        let salt = Salt::new(salt.data().try_into().unwrap());

        let tx_id = match iter.next().unwrap().1 {
            Field::Bytes(tx_id) => Some(tx_id),
            Field::Null => None,
            _ => panic!("Should not happen"),
        };
        let tx_id = tx_id.map(|data| Bytes32::new(data.data().try_into().unwrap()));

        let output_index = match iter.next().unwrap().1 {
            Field::UByte(output_index) => Some(*output_index),
            Field::Null => None,
            _ => panic!("Should not happen"),
        };

        let tx_pointer_block_height = match iter.next().unwrap().1 {
            Field::UInt(tx_pointer_block_height) => Some(*tx_pointer_block_height),
            Field::Null => None,
            _ => panic!("Should not happen"),
        };
        let tx_pointer_block_height = tx_pointer_block_height.map(BlockHeight::new);

        let tx_pointer_tx_idx = match iter.next().unwrap().1 {
            Field::UShort(tx_pointer_tx_idx) => Some(*tx_pointer_tx_idx),
            Field::Null => None,
            _ => panic!("Should not happen"),
        };
        Self {
            contract_id,
            code,
            salt,
            tx_id,
            output_index,
            tx_pointer_block_height,
            tx_pointer_tx_idx,
            state: None,
            balances: None,
        }
    }
}
