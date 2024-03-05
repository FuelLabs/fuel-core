use std::marker::PhantomData;

use anyhow::{
    anyhow,
    Context,
};
use parquet::{
    data_type::AsBytes,
    file::{
        reader::{
            ChunkReader,
            FileReader,
        },
        serialized_reader::SerializedFileReader,
    },
    record::RowAccessor,
};

use crate::config::state::{
    Group,
    GroupResult,
};

pub type PostcardDecoder<T> = Decoder<std::fs::File, T, PostcardDecode>;

pub struct Decoder<R: ChunkReader, T, D> {
    data_source: SerializedFileReader<R>,
    group_index: usize,
    _data: PhantomData<T>,
    _decoder: PhantomData<D>,
}

pub trait Decode<T> {
    fn decode(bytes: &[u8]) -> anyhow::Result<T>
    where
        Self: Sized;
}

impl<R, T, D> Decoder<R, T, D>
where
    R: ChunkReader + 'static,
    D: Decode<T>,
{
    fn current_group(&self) -> anyhow::Result<Group<T>> {
        let data = self
            .data_source
            .get_row_group(self.group_index)?
            .get_row_iter(None)?
            .map(|result| {
                result.map_err(|e| anyhow!(e)).and_then(|row| {
                    const FIELD_IDX: usize = 0;
                    let bytes = row
                        .get_bytes(FIELD_IDX)
                        .context("While decoding postcard bytes")?
                        .as_bytes();
                    D::decode(bytes)
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Group {
            index: self.group_index,
            data,
        })
    }
}

impl<R, T, D> Iterator for Decoder<R, T, D>
where
    R: ChunkReader + 'static,
    D: Decode<T>,
{
    type Item = GroupResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.group_index >= self.data_source.metadata().num_row_groups() {
            return None;
        }

        let group = self.current_group();
        self.group_index = self.group_index.saturating_add(1);

        Some(group)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.group_index = self.group_index.saturating_add(n);
        self.next()
    }
}

impl<R: ChunkReader + 'static, T, D> Decoder<R, T, D> {
    pub fn new(reader: R) -> anyhow::Result<Self> {
        Ok(Self {
            data_source: SerializedFileReader::new(reader)?,
            group_index: 0,
            _data: PhantomData,
            _decoder: PhantomData,
        })
    }
}

pub struct PostcardDecode;

impl<T> Decode<T> for PostcardDecode
where
    T: serde::de::DeserializeOwned,
{
    fn decode(bytes: &[u8]) -> anyhow::Result<T>
    where
        Self: Sized,
    {
        Ok(postcard::from_bytes(bytes)?)
    }
}
