use std::{
    io::Write,
    marker::PhantomData,
    sync::Arc,
};

use itertools::Itertools;
use parquet::{
    basic::{
        Compression,
        Repetition,
    },
    file::{
        properties::WriterProperties,
        writer::SerializedFileWriter,
    },
};

use parquet::{
    data_type::ByteArrayType,
    schema::types::Type,
};
use postcard::ser_flavors::{
    AllocVec,
    Flavor,
};

pub type PostcardEncoder<T> = Encoder<std::fs::File, T, PostcardEncode>;

pub struct Encoder<W: Write + Send, T, E> {
    writer: SerializedFileWriter<W>,
    _type: PhantomData<T>,
    _encoding: PhantomData<E>,
}

impl<W: Write + Send, T, E> Encoder<W, T, E>
where
    E: Encode<T>,
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

        Ok(Self {
            writer,
            _type: PhantomData,
            _encoding: PhantomData,
        })
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

impl<W, T, E> Encoder<W, T, E>
where
    W: Write + Send,
    E: Encode<T>,
{
    pub fn write(&mut self, elements: Vec<T>) -> anyhow::Result<()> {
        let mut group = self.writer.next_row_group()?;
        let mut column = group
            .next_column()?
            .ok_or_else(|| anyhow::anyhow!("Missing column. Check the schema!"))?;

        let values: Vec<_> = elements
            .into_iter()
            .map(|el| E::encode(&el))
            .map_ok(Into::into)
            .try_collect()?;
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

pub trait Encode<T> {
    fn encode(data: &T) -> anyhow::Result<Vec<u8>>;
}

pub struct PostcardEncode;

impl<T> Encode<T> for PostcardEncode
where
    T: serde::Serialize,
{
    fn encode(data: &T) -> anyhow::Result<Vec<u8>> {
        let mut serializer = postcard::Serializer {
            output: AllocVec::new(),
        };
        data.serialize(&mut serializer)?;
        Ok(serializer.output.finalize()?)
    }
}
