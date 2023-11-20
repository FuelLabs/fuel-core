use anyhow::anyhow;
use std::{io::Write, sync::Arc};

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
        codec::StateEncoder, contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig,
};

use super::schema::Schema;

pub struct Encoder<W: Write> {
    coins: SerializedFileWriter<W>,
    messages: SerializedFileWriter<W>,
    contracts: SerializedFileWriter<W>,
    contract_state: SerializedFileWriter<W>,
    contract_balance: SerializedFileWriter<W>,
}

impl<W: Write + Send> Encoder<W> {
    pub fn new(
        coins: W,
        messages: W,
        contracts: W,
        contract_state: W,
        contract_balance: W,
        compression: Compression,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            coins: Self::get_writer::<CoinConfig>(coins, compression)?,
            messages: Self::get_writer::<MessageConfig>(messages, compression)?,
            contracts: Self::get_writer::<ContractConfig>(contracts, compression)?,
            contract_state: Self::get_writer::<ContractState>(
                contract_state,
                compression,
            )?,
            contract_balance: Self::get_writer::<ContractBalance>(
                contract_balance,
                compression,
            )?,
        })
    }

    fn get_writer<T: Schema>(
        writer: W,
        compression: Compression,
    ) -> anyhow::Result<SerializedFileWriter<W>> {
        Ok(SerializedFileWriter::new(
            writer,
            Arc::new(T::schema()),
            Arc::new(
                WriterProperties::builder()
                    .set_compression(compression)
                    .build(),
            ),
        )?)
    }
}

impl<W> StateEncoder for Encoder<W>
where
    W: Write + Send,
{
    fn close(self: Box<Self>) -> anyhow::Result<()> {
        self.coins.close()?;
        self.contracts.close()?;
        self.messages.close()?;
        self.contract_state.close()?;
        self.contract_balance.close()?;

        Ok(())
    }

    fn write_coins(&mut self, elements: Vec<CoinConfig>) -> anyhow::Result<()> {
        elements.encode_columns(&mut self.coins)
    }

    fn write_contracts(&mut self, elements: Vec<ContractConfig>) -> anyhow::Result<()> {
        elements.encode_columns(&mut self.contracts)
    }

    fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        elements.encode_columns(&mut self.messages)
    }

    fn write_contract_state(
        &mut self,
        elements: Vec<ContractState>,
    ) -> anyhow::Result<()> {
        elements.encode_columns(&mut self.contract_state)
    }

    fn write_contract_balance(
        &mut self,
        elements: Vec<ContractBalance>,
    ) -> anyhow::Result<()> {
        elements.encode_columns(&mut self.contract_balance)
    }
}

pub trait ColumnEncoder {
    type ElementT: Schema;
    fn encode_columns<W: std::io::Write + Send>(
        &self,
        writer: &mut SerializedFileWriter<W>,
    ) -> anyhow::Result<()> {
        let mut group = writer.next_row_group()?;

        let num_of_columns = <Self::ElementT>::num_of_columns();
        for index in 0..num_of_columns {
            let mut column = group.next_column()?.ok_or_else(|| anyhow!("Error fetching column({index}). Expected there to be {num_of_columns} columns."))?;
            self.encode_column(index, &mut column)?;
            column.close()?;
        }

        group.close()?;
        Ok(())
    }

    fn encode_column(
        &self,
        index: usize,
        column: &mut SerializedColumnWriter<'_>,
    ) -> anyhow::Result<()>;
}

impl ColumnEncoder for Vec<ContractConfig> {
    type ElementT = ContractConfig;

    fn encode_column(
        &self,
        index: usize,
        column: &mut SerializedColumnWriter<'_>,
    ) -> anyhow::Result<()> {
        match index {
            0 => {
                let data = self
                    .iter()
                    .map(|el| el.contract_id.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            1 => {
                let data = self.iter().map(|el| el.code.clone().into()).collect_vec();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            2 => {
                let data = self.iter().map(|el| el.salt.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            3 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.tx_id.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_id)
                    .map(|el| el.to_vec().into())
                    .collect_vec();
                column.typed::<FixedLenByteArrayType>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            4 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.output_index.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.output_index)
                    .map(i32::from)
                    .collect_vec();
                column.typed::<Int32Type>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            5 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.tx_pointer_block_height.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_pointer_block_height)
                    .map(|el| *el as i32)
                    .collect_vec();
                column.typed::<Int32Type>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            6 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.tx_pointer_tx_idx.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_pointer_tx_idx)
                    .map(i32::from)
                    .collect_vec();
                column.typed::<Int32Type>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
        Ok(())
    }
}

impl ColumnEncoder for Vec<CoinConfig> {
    type ElementT = CoinConfig;

    fn encode_column(
        &self,
        index: usize,
        column: &mut SerializedColumnWriter<'_>,
    ) -> anyhow::Result<()> {
        match index {
            0 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.tx_id.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_id)
                    .map(|el| el.to_vec().into())
                    .collect_vec();
                column.typed::<FixedLenByteArrayType>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            1 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.output_index.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.output_index)
                    .map(i32::from)
                    .collect_vec();
                column.typed::<Int32Type>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            2 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.tx_pointer_block_height.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_pointer_block_height)
                    .map(|el| *el as i32)
                    .collect_vec();
                column.typed::<Int32Type>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            3 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.tx_pointer_tx_idx.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.tx_pointer_tx_idx)
                    .map(i32::from)
                    .collect_vec();
                column.typed::<Int32Type>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            4 => {
                let def_levels = self
                    .iter()
                    .map(|el| i16::from(el.maturity.is_some()))
                    .collect_vec();
                let data = self
                    .iter()
                    .filter_map(|el| el.maturity)
                    .map(|el| *el as i32)
                    .collect_vec();
                column.typed::<Int32Type>().write_batch(
                    &data,
                    Some(&def_levels),
                    None,
                )?;
            }
            5 => {
                let data = self.iter().map(|el| el.owner.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            6 => {
                let data = self.iter().map(|el| el.amount as i64).collect_vec();
                column.typed::<Int64Type>().write_batch(&data, None, None)?;
            }
            7 => {
                let data = self
                    .iter()
                    .map(|el| el.asset_id.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
        Ok(())
    }
}

impl ColumnEncoder for Vec<MessageConfig> {
    type ElementT = MessageConfig;

    fn encode_column(
        &self,
        index: usize,
        column: &mut SerializedColumnWriter<'_>,
    ) -> anyhow::Result<()> {
        match index {
            0 => {
                let data = self
                    .iter()
                    .map(|el| el.sender.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            1 => {
                let data = self
                    .iter()
                    .map(|el| el.recipient.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            2 => {
                let data = self.iter().map(|el| el.nonce.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            3 => {
                let data = self.iter().map(|el| el.amount as i64).collect_vec();
                column.typed::<Int64Type>().write_batch(&data, None, None)?;
            }
            4 => {
                let data = self.iter().map(|el| el.data.clone().into()).collect_vec();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            5 => {
                let data = self.iter().map(|el| el.da_height.0 as i64).collect_vec();
                column.typed::<Int64Type>().write_batch(&data, None, None)?;
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
        Ok(())
    }
}

impl ColumnEncoder for Vec<ContractState> {
    type ElementT = ContractState;

    fn encode_column(
        &self,
        index: usize,
        column: &mut SerializedColumnWriter<'_>,
    ) -> anyhow::Result<()> {
        match index {
            0 => {
                let data = self
                    .iter()
                    .map(|el| el.contract_id.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            1 => {
                let data = self.iter().map(|el| el.key.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            2 => {
                let data = self.iter().map(|el| el.value.to_vec().into()).collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
        Ok(())
    }
}

impl ColumnEncoder for Vec<ContractBalance> {
    type ElementT = ContractBalance;

    fn encode_column(
        &self,
        index: usize,
        column: &mut SerializedColumnWriter<'_>,
    ) -> anyhow::Result<()> {
        match index {
            0 => {
                let data = self
                    .iter()
                    .map(|el| el.contract_id.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            1 => {
                let data = self
                    .iter()
                    .map(|el| el.asset_id.to_vec().into())
                    .collect_vec();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&data, None, None)?;
            }
            2 => {
                let data = self.iter().map(|el| el.amount as i64).collect_vec();
                column.typed::<Int64Type>().write_batch(&data, None, None)?;
            }
            unknown_column => {
                panic!(
                    "Unknown column {unknown_column}, doesn't index schema: {:?}",
                    <Self::ElementT>::schema()
                )
            }
        }
        Ok(())
    }
}
