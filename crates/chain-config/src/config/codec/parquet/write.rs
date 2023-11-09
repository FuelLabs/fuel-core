use std::{io::Write, marker::PhantomData, sync::Arc};

use itertools::Itertools;
use parquet::{
    basic::Compression,
    data_type::{ByteArrayType, FixedLenByteArrayType, Int32Type, Int64Type},
    file::{
        properties::WriterProperties,
        writer::{SerializedColumnWriter, SerializedFileWriter},
    },
};

use crate::{
    config::{
        codec::BatchWriter, contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig,
};

use super::schema::ParquetSchema;

pub(crate) struct ParquetBatchWriter<W: Write, T> {
    writer: SerializedFileWriter<W>,
    _data: PhantomData<T>,
}

impl<W: Write + Send, T: ParquetSchema> ParquetBatchWriter<W, T> {
    pub fn new(writer: W, compression: Compression) -> anyhow::Result<Self> {
        let writer = SerializedFileWriter::new(
            writer,
            Arc::new(T::schema()),
            Arc::new(
                WriterProperties::builder()
                    .set_compression(compression)
                    .build(),
            ),
        )
        .unwrap();

        Ok(Self {
            writer,
            _data: PhantomData,
        })
    }
}

impl<W: Write + Send, T> ParquetBatchWriter<W, T> {
    pub fn into_inner(self) -> W {
        self.writer.into_inner().unwrap()
    }
}

impl<T, W> BatchWriter<T> for ParquetBatchWriter<W, T>
where
    T: ParquetSchema,
    Vec<T>: ColumnEncoder,
    W: Write + Send,
{
    fn write_batch(&mut self, elements: Vec<T>) -> anyhow::Result<()> {
        elements.encode_columns(&mut self.writer);
        Ok(())
    }
}

trait ColumnEncoder {
    type ElementT: ParquetSchema;
    fn encode_columns<W: std::io::Write + Send>(
        &self,
        writer: &mut SerializedFileWriter<W>,
    ) {
        let mut group = writer.next_row_group().unwrap();

        for index in 0..<Self::ElementT>::num_of_columns() {
            let mut column = group.next_column().unwrap().unwrap();
            self.encode_column(index, &mut column);
            column.close().unwrap();
        }

        group.close().unwrap();
    }
    fn encode_column(&self, index: usize, column: &mut SerializedColumnWriter<'_>);
}

impl ColumnEncoder for Vec<ContractConfig> {
    type ElementT = ContractConfig;

    fn encode_column(&self, index: usize, column: &mut SerializedColumnWriter<'_>) {
        match index {
            0 => {
                let data = self
                    .iter()
                    .map(|el| el.contract_id.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            1 => {
                let data = self.iter().map(|el| el.code.clone().into()).collect_vec();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            2 => {
                let data = self.iter().map(|el| el.salt.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            3 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.tx_id.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_id)
                    .map(|el| el.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            4 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.output_index.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.output_index)
                    .map(|el| el as i32)
                    .collect_vec();
                column
                    .typed::<Int32Type>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            5 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.tx_pointer_block_height.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_pointer_block_height)
                    .map(|el| *el as i32)
                    .collect_vec();
                column
                    .typed::<Int32Type>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            6 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.tx_pointer_tx_idx.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_pointer_tx_idx)
                    .map(|el| el as i32)
                    .collect_vec();
                column
                    .typed::<Int32Type>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
    }
}

impl ColumnEncoder for Vec<CoinConfig> {
    type ElementT = CoinConfig;

    fn encode_column(&self, index: usize, column: &mut SerializedColumnWriter<'_>) {
        match index {
            0 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.tx_id.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_id)
                    .map(|el| el.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            1 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.output_index.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.output_index)
                    .map(|el| el as i32)
                    .collect_vec();
                column
                    .typed::<Int32Type>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            2 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.tx_pointer_block_height.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_pointer_block_height)
                    .map(|el| *el as i32)
                    .collect_vec();
                column
                    .typed::<Int32Type>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            3 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.tx_pointer_tx_idx.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_pointer_tx_idx)
                    .map(|el| el as i32)
                    .collect_vec();
                column
                    .typed::<Int32Type>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            4 => {
                let def_levels = self
                    .iter()
                    .map(|el| el.maturity.is_some() as i16)
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.maturity)
                    .map(|el| *el as i32)
                    .collect_vec();
                column
                    .typed::<Int32Type>()
                    .write_batch(&data, Some(&def_levels), None)
                    .unwrap();
            }
            5 => {
                let data = self.iter().map(|el| el.owner.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            6 => {
                let data = self.iter().map(|el| el.amount as i64).collect_vec();
                column
                    .typed::<Int64Type>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            7 => {
                let data = self
                    .iter()
                    .map(|el| el.asset_id.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
    }
}

impl ColumnEncoder for Vec<MessageConfig> {
    type ElementT = MessageConfig;

    fn encode_column(&self, index: usize, column: &mut SerializedColumnWriter<'_>) {
        match index {
            0 => {
                let data = self
                    .iter()
                    .map(|el| el.sender.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            1 => {
                let data = self
                    .iter()
                    .map(|el| el.recipient.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            2 => {
                let data = self.iter().map(|el| el.nonce.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            3 => {
                let data = self.iter().map(|el| el.amount as i64).collect_vec();
                column
                    .typed::<Int64Type>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            4 => {
                let data = self.iter().map(|el| el.data.to_vec().into()).collect_vec();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            5 => {
                let data = self.iter().map(|el| el.da_height.0 as i64).collect_vec();
                column
                    .typed::<Int64Type>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
    }
}

impl ColumnEncoder for Vec<ContractState> {
    type ElementT = ContractState;

    fn encode_column(&self, index: usize, column: &mut SerializedColumnWriter<'_>) {
        match index {
            0 => {
                let data = self.iter().map(|el| el.key.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            1 => {
                let data = self.iter().map(|el| el.value.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
    }
}

impl ColumnEncoder for Vec<ContractBalance> {
    type ElementT = ContractBalance;

    fn encode_column(&self, index: usize, column: &mut SerializedColumnWriter<'_>) {
        match index {
            0 => {
                let data = self
                    .iter()
                    .map(|el| el.asset_id.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            1 => {
                let data = self.iter().map(|el| el.amount as i64).collect_vec();
                column
                    .typed::<Int64Type>()
                    .write_batch(&data, None, None)
                    .unwrap();
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
    }
}
