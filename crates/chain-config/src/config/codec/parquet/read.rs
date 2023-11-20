use std::marker::PhantomData;

use anyhow::{anyhow, bail, Context};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::{Address, AssetId, BlockHeight, Bytes32, ContractId, Nonce},
    fuel_vm::Salt,
};
use parquet::{
    file::{
        reader::{ChunkReader, FileReader},
        serialized_reader::SerializedFileReader,
    },
    record::{Field, Row},
};

use crate::{
    config::{
        codec::{Group, GroupDecoder, GroupResult},
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig,
};

pub struct ParquetBatchReader<R: ChunkReader, T> {
    data_source: SerializedFileReader<R>,
    group_index: usize,
    _data: PhantomData<T>,
}

impl<R, T> ParquetBatchReader<R, T>
where
    R: ChunkReader + 'static,
    T: TryFrom<Row, Error = anyhow::Error>,
{
    fn get_group(&self, index: usize) -> anyhow::Result<Group<T>> {
        let data = self
            .data_source
            .get_row_group(self.group_index)?
            .get_row_iter(None)?
            .map(|result| {
                result
                    .map_err(|e| anyhow!(e))
                    .and_then(|row| T::try_from(row))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Group { index, data })
    }
}
impl<R, T> GroupDecoder<T> for ParquetBatchReader<R, T>
where
    R: ChunkReader + 'static,
    T: TryFrom<Row, Error = anyhow::Error>,
{
}

impl<R, T> Iterator for ParquetBatchReader<R, T>
where
    R: ChunkReader + 'static,
    T: TryFrom<Row, Error = anyhow::Error>,
{
    type Item = GroupResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.group_index >= self.data_source.metadata().num_row_groups() {
            return None;
        }

        let group_index = self.group_index;

        let group = self.get_group(group_index);
        self.group_index += 1;

        Some(group)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.group_index = n;
        self.next()
    }
}

impl<R: ChunkReader + 'static, T> ParquetBatchReader<R, T> {
    pub fn new(reader: R) -> anyhow::Result<Self> {
        Ok(Self {
            data_source: SerializedFileReader::new(reader)?,
            group_index: 0,
            _data: PhantomData,
        })
    }
}

impl TryFrom<Row> for CoinConfig {
    type Error = anyhow::Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let mut iter = row.get_column_iter();
        let mut next_field = || {
            iter.next()
                .map(|el| el.1)
                .ok_or_else(|| anyhow!("Expected at least one more field. Row: {row:?}"))
        };

        let tx_id = next_field()
            .and_then(decode_as_optional_bytes_32)
            .context("While decoding `tx_id`")?;

        let output_index = next_field()
            .and_then(decode_as_optional_u8)
            .context("While decoding `output_index`")?;

        let tx_pointer_block_height = next_field()
            .and_then(decode_as_optional_block_height)
            .context("While decoding `tx_pointer_block_height`")?;

        let tx_pointer_tx_idx = next_field()
            .and_then(decode_as_optional_u16)
            .context("While decoding `tx_pointer_tx_idx`")?;

        let maturity = next_field()
            .and_then(decode_as_optional_block_height)
            .context("While decoding `maturiy`")?;

        let owner = next_field()
            .and_then(decode_as_address)
            .context("While decoding `owner`")?;

        let amount = next_field()
            .and_then(decode_as_u64)
            .context("While decoding `amount`")?;

        let asset_id = next_field()
            .and_then(decode_as_asset_id)
            .context("While decoding `assert_id`")?;

        Ok(Self {
            tx_id,
            output_index,
            tx_pointer_block_height,
            tx_pointer_tx_idx,
            maturity,
            owner,
            amount,
            asset_id,
        })
    }
}

fn decode_as_bytes(field: &Field) -> anyhow::Result<&[u8]> {
    decode_as_optional_bytes(field)?.ok_or_else(|| anyhow!("Cannot be NULL"))
}

fn decode_as_optional_bytes(field: &Field) -> anyhow::Result<Option<&[u8]>> {
    let bytes = match field {
        Field::Bytes(bytes) => Some(bytes.data()),
        Field::Null => None,
        field => bail!("Unexpected field: '{field}'"),
    };
    Ok(bytes)
}

fn decode_as_optional_u8(field: &Field) -> anyhow::Result<Option<u8>> {
    let bytes = match field {
        Field::UByte(val) => Some(*val),
        Field::Null => None,
        field => bail!("Unexpected field: '{field}'"),
    };
    Ok(bytes)
}

fn decode_as_optional_u16(field: &Field) -> anyhow::Result<Option<u16>> {
    let val = match field {
        Field::UShort(val) => Some(*val),
        Field::Null => None,
        field => bail!("Unexpected field: '{field}'"),
    };
    Ok(val)
}

fn decode_as_optional_u64(field: &Field) -> anyhow::Result<Option<u64>> {
    let val = match field {
        Field::ULong(val) => Some(*val),
        Field::Null => None,
        field => bail!("Unexpected field: '{field}'"),
    };
    Ok(val)
}

fn decode_as_u64(field: &Field) -> anyhow::Result<u64> {
    decode_as_optional_u64(field)?.ok_or_else(|| anyhow!("Cannot be NULL"))
}

fn decode_as_optional_asset_id(field: &Field) -> anyhow::Result<Option<AssetId>> {
    Ok(decode_as_optional_bytes_32(field)?.map(|bytes_32| AssetId::new(*bytes_32)))
}

fn decode_as_asset_id(field: &Field) -> anyhow::Result<AssetId> {
    decode_as_optional_asset_id(field)?.ok_or_else(|| anyhow!("Cannot be NULL"))
}

fn decode_as_bytes_32(field: &Field) -> anyhow::Result<Bytes32> {
    decode_as_optional_bytes_32(field)?.ok_or_else(|| anyhow!("Cannot be NULL"))
}

fn decode_as_optional_bytes_32(field: &Field) -> anyhow::Result<Option<Bytes32>> {
    let bytes = decode_as_optional_bytes(field)?;
    bytes
        .map(|bytes| -> anyhow::Result<_> {
            let data = bytes.try_into()?;
            Ok(Bytes32::new(data))
        })
        .transpose()
}

fn decode_as_optional_block_height(field: &Field) -> anyhow::Result<Option<BlockHeight>> {
    let val = match field {
        Field::UInt(val) => Some(*val),
        Field::Null => None,
        field => bail!("Unexpected field: '{field}'"),
    };
    Ok(val.map(BlockHeight::new))
}

fn decode_as_optional_address(field: &Field) -> anyhow::Result<Option<Address>> {
    Ok(decode_as_optional_bytes_32(field)?.map(|bytes_32| Address::new(*bytes_32)))
}

fn decode_as_address(field: &Field) -> anyhow::Result<Address> {
    decode_as_optional_address(field)?.ok_or_else(|| anyhow!("Cannot be NULL"))
}

impl TryFrom<Row> for MessageConfig {
    type Error = anyhow::Error;
    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let mut iter = row.get_column_iter();
        let mut next_field = || {
            iter.next()
                .map(|el| el.1)
                .ok_or_else(|| anyhow!("Expected at least one more field. Row: {row:?}"))
        };

        let sender = next_field()
            .and_then(decode_as_address)
            .context("While decoding `sender`")?;

        let recipient = next_field()
            .and_then(decode_as_address)
            .context("While decoding `recipient`")?;

        let nonce = next_field()
            .and_then(decode_as_bytes_32)
            .map(|bytes_32| Nonce::new(*bytes_32))
            .context("While decoding 'nonce'")?;

        let amount = next_field()
            .and_then(decode_as_u64)
            .context("While decoding `amount`")?;

        let data = next_field()
            .and_then(decode_as_bytes)
            .context("While decoding `data`")?
            .to_vec();

        let da_height = next_field()
            .and_then(decode_as_u64)
            .map(DaBlockHeight)
            .context("While decoding `amount`")?;

        Ok(Self {
            sender,
            recipient,
            nonce,
            amount,
            data,
            da_height,
        })
    }
}

impl TryFrom<Row> for ContractState {
    type Error = anyhow::Error;
    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let mut iter = row.get_column_iter();
        let mut next_field = || {
            iter.next()
                .map(|el| el.1)
                .ok_or_else(|| anyhow!("Expected at least one more field. Row: {row:?}"))
        };

        let contract_id = next_field()
            .and_then(decode_as_bytes_32)
            .context("While decoding `contract_id`")?;

        let key = next_field()
            .and_then(decode_as_bytes_32)
            .context("While decoding `key`")?;

        let value = next_field()
            .and_then(decode_as_bytes_32)
            .context("While decoding `value`")?;

        Ok(Self {
            contract_id,
            key,
            value,
        })
    }
}

impl TryFrom<Row> for ContractBalance {
    type Error = anyhow::Error;
    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let mut iter = row.get_column_iter();
        let mut next_field = || {
            iter.next()
                .map(|el| el.1)
                .ok_or_else(|| anyhow!("Expected at least one more field. Row: {row:?}"))
        };

        let contract_id = next_field()
            .and_then(decode_as_bytes_32)
            .context("While decoding `contract_id`")?;

        let asset_id = next_field()
            .and_then(decode_as_bytes_32)
            .map(|bytes_32| AssetId::new(*bytes_32))
            .context("While decoding `contract_id`")?;

        let amount = next_field()
            .and_then(decode_as_u64)
            .context("While decoding `amount`")?;

        Ok(Self {
            contract_id,
            asset_id,
            amount,
        })
    }
}

impl TryFrom<Row> for ContractConfig {
    type Error = anyhow::Error;
    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let mut iter = row.get_column_iter();
        let mut next_field = || {
            iter.next()
                .map(|el| el.1)
                .ok_or_else(|| anyhow!("Expected at least one more field. Row: {row:?}"))
        };

        let contract_id = next_field()
            .and_then(decode_as_bytes_32)
            .map(|bytes_32| ContractId::new(*bytes_32))
            .context("While decoding `contract_id`")?;

        let code = next_field()
            .and_then(decode_as_bytes)
            .context("While decoding `code`")?
            .to_vec();

        let salt = next_field()
            .and_then(decode_as_bytes_32)
            .map(|bytes_32| Salt::new(*bytes_32))
            .context("While decoding `salt`")?;

        let tx_id = next_field()
            .and_then(decode_as_optional_bytes_32)
            .context("While decoding `tx_id`")?;

        let output_index = next_field()
            .and_then(decode_as_optional_u8)
            .context("While decoding `output_index`")?;

        let tx_pointer_block_height = next_field()
            .and_then(decode_as_optional_block_height)
            .context("While decoding `tx_pointer_block_height`")?;

        let tx_pointer_tx_idx = next_field()
            .and_then(decode_as_optional_u16)
            .context("While decoding `tx_pointer_tx_idx`")?;

        Ok(Self {
            contract_id,
            code,
            salt,
            tx_id,
            output_index,
            tx_pointer_block_height,
            tx_pointer_tx_idx,
            state: None,
            balances: None,
        })
    }
}
