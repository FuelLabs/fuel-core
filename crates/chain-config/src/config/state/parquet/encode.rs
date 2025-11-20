use std::{io::Write, sync::Arc};

use itertools::Itertools;
use parquet::{
    basic::{Compression, Repetition},
    file::{properties::WriterProperties, writer::SerializedFileWriter},
};

use parquet::{data_type::ByteArrayType, schema::types::Type};

pub struct Encoder<W>
where
    W: Write,
{
    writer: SerializedFileWriter<W>,
}

impl<W> Encoder<W>
where
    W: Write + Send,
{
    pub fn new(writer: W, compression: Compression) -> anyhow::Result<Self> {
        let writer = SerializedFileWriter::new(
            writer,
            Arc::new(Self::single_element_schema()),
            Arc::new(
                WriterProperties::builder()
                    .set_compression(compression)
                    .build(),
            ),
        )?;

        Ok(Self { writer })
    }

    fn single_element_schema() -> Type {
        let data =
            Type::primitive_type_builder("data", ::parquet::basic::Type::BYTE_ARRAY)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .expect("This is a valid schema");

        Type::group_type_builder("unimportant")
            .with_fields(vec![Arc::new(data)])
            .build()
            .expect("This is a valid schema")
    }
}

impl<W> Encoder<W>
where
    W: Write + Send,
{
    pub fn write(&mut self, elements: Vec<Vec<u8>>) -> anyhow::Result<()> {
        let mut group = self.writer.next_row_group()?;
        let mut column = group
            .next_column()?
            .ok_or_else(|| anyhow::anyhow!("Missing column. Check the schema!"))?;

        let values = elements.into_iter().map(Into::into).collect_vec();
        column
            .typed::<ByteArrayType>()
            .write_batch(&values, None, None)?;

        column.close()?;
        group.close()?;
        Ok(())
    }

    pub fn close(self) -> anyhow::Result<()> {
        self.writer.close()?;
        Ok(())
    }
}
