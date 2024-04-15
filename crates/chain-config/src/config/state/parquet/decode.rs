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

pub struct Decoder<R: ChunkReader> {
    data_source: SerializedFileReader<R>,
    group_index: usize,
}

impl<R> Decoder<R>
where
    R: ChunkReader + 'static,
{
    pub fn num_groups(&self) -> usize {
        self.data_source.num_row_groups()
    }

    fn current_group(&self) -> anyhow::Result<Vec<Vec<u8>>> {
        let data = self
            .data_source
            .get_row_group(self.group_index)?
            .get_row_iter(None)?
            .map(|result| {
                result.map_err(|e| anyhow!(e)).and_then(|row| {
                    const FIELD_IDX: usize = 0;
                    Ok(row
                        .get_bytes(FIELD_IDX)
                        .context("While decoding postcard bytes")?
                        .as_bytes()
                        .to_vec())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(data)
    }
}

impl<R> Iterator for Decoder<R>
where
    R: ChunkReader + 'static,
{
    type Item = anyhow::Result<Vec<Vec<u8>>>;

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

impl<R: ChunkReader + 'static> Decoder<R> {
    pub fn new(reader: R) -> anyhow::Result<Self> {
        Ok(Self {
            data_source: SerializedFileReader::new(reader)?,
            group_index: 0,
        })
    }
}
