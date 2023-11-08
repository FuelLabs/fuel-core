use std::{
    marker::PhantomData,
    path::Path, io::{Write, BufWriter, Cursor, Read, Seek, BufReader, IntoInnerError, BufRead}, sync::{Arc, atomic::AtomicU64},
};
use bincode::config::{Configuration, LittleEndian, NoLimit, Varint};

use fuel_core_types::fuel_types::{
    Bytes32,
    ContractId,
};
use itertools::Itertools;
use serde::de::DeserializeOwned;

use crate::{
    CoinConfig,
    ContractConfig,
    MessageConfig,
};

#[derive(Debug)]
struct TrackingWriter<T: Debug> {
    writer: T,
    written_bytes: Arc<AtomicU64>,
}

impl<T: Debug> TrackingWriter<T> {
    pub fn new(writer: T) -> Self {
        Self {
            writer,
            written_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn written_bytes(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.written_bytes)
    }

    pub fn into_inner(self) -> T {
        self.writer
    }
}

impl<T: Write + Debug> Write for TrackingWriter<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.writer.write(buf)?;
        self.written_bytes
            .fetch_add(written as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

struct InMemorySource {
    // The encoded data inside a `Cursor`. Note this is not our cursor i.e. progress tracker, but
    // rather something rust provides so that you may mimic a file using only a Vec<u8>
    data: Cursor<Vec<u8>>,
    // also has a handy field containing the cursors of all batches encoded in `self.data`. Useful
    // for testing
    element_cursors: Vec<u64>,
}

impl InMemorySource {
    pub fn new<T: serde::Serialize>(
        entries: impl IntoIterator<Item = T>,
        batch_size: usize,
    ) -> std::io::Result<Self> {
        let buffer = Cursor::new(vec![]);

        let writer = TrackingWriter::new(buffer);
        // this allows us to give up ownership of `writer` but still be able to peek inside it
        let bytes_written = writer.written_bytes();

        let mut writer = StateWriter::new(writer);
        let element_cursors = entries
            .into_iter()
            .chunks(batch_size)
            .into_iter()
            .map(|chunk| {
                // remember the starting position
                let cursor = bytes_written.load(std::sync::atomic::Ordering::Relaxed);
                writer.write_batch(chunk.collect_vec()).unwrap();
                // since `GenericWriter` has a buffered writer inside of it, it won't flush all the
                // time. This is bad for us here since we want all the data flushed to our
                // `TrackingWriter` so that it may count the bytes. We use that count to provide
                // the cursors for each batch -- useful for testing.
                writer.flush().unwrap();
                cursor
            })
            .collect();

        Ok(Self {
            // basically unpeals the writers, first we get the tracking writer, then we get the
            // Cursor we gave it. into_inner will flush so we can be sure that the final Cursor has
            // all the data. Also we did a bunch of flushing above
            data: writer.into_inner()?.into_inner(),
            element_cursors,
        })
    }

    // useful for tests so we don't have to hardcode boundaries
    pub fn batch_cursors(&self) -> &[u64] {
        &self.element_cursors
    }
}

impl Read for InMemorySource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.data.read(buf)
    }
}

impl Seek for InMemorySource {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.data.seek(pos)
    }
}

// So that we may record how much was written before we give it to `source`
struct TrackingBuffReader<T> {
    amount_read: u64,
    source: BufReader<T>,
}

impl<T: Seek> Seek for TrackingBuffReader<T> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.source.seek(pos)
    }
}

impl<T: Read> TrackingBuffReader<T> {
    pub fn new(source: T) -> Self {
        Self {
            amount_read: 0,
            source: BufReader::new(source),
        }
    }

    // unfortunately this is the best way i can find to check if there is any more data. the actual
    // `has_data_left` method is yet to be stabilized
    pub fn has_data_left(&mut self) -> std::io::Result<bool> {
        Ok(!self.source.fill_buf()?.is_empty())
    }
}

impl<T: Read> Read for TrackingBuffReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let amount = self.source.read(buf)?;
        self.amount_read += amount as u64;
        Ok(amount)
    }
}

pub struct StateReader<R> {
    source: TrackingBuffReader<R>,
}

impl<R: Read + Seek> StateReader<R> {
    pub fn new(source: R, start_cursor: u64) -> std::io::Result<Self> {
        let mut reader = TrackingBuffReader::new(source);
        reader.seek(std::io::SeekFrom::Start(start_cursor))?;
        Ok(Self { source: reader })
    }

    pub fn batch_cursor(&self) -> u64 {
        self.source.amount_read
    }

    pub fn read_batch<T: DeserializeOwned>(&mut self) -> anyhow::Result<Vec<T>> {
        let coins = if self.source.has_data_left()? {
            bincode::serde::decode_from_std_read(
                &mut self.source,
                Configuration::<LittleEndian, Varint, NoLimit>::default(),
            )?
        } else {
            vec![]
        };

        Ok(coins)
    }
}

struct StateWriter<W: Write> {
    dest: BufWriter<W>,
}

use std::fmt::Debug;
impl<W: Write + Debug> StateWriter<W> {
    pub fn new(dest: W) -> Self {
        Self {
            dest: BufWriter::new(dest),
        }
    }

    pub fn write_batch(&mut self, coins: Vec<impl serde::Serialize>) -> anyhow::Result<()> {
        bincode::serde::encode_into_std_write(
            coins,
            &mut self.dest,
            Configuration::<LittleEndian, Varint, NoLimit>::default(),
        )?;

        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.dest.flush()
    }

    pub fn into_inner(self) -> Result<W, IntoInnerError<BufWriter<W>>> {
        self.dest.into_inner()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        /*
        let mut importer = StateImporter::<CoinConfig> {
            phantom_data: std::marker::PhantomData,
        };

        let next_item = importer.next();
        let cursor = importer.current_cursor();

        for coin in importer {}

        let mut importer = importer.messages();

        let next_item = importer.next();
        let cursor = importer.current_cursor();

        for message in importer {}

        let mut importer = importer.contracts();

        let next_item = importer.next();
        let cursor = importer.current_cursor();

        for contract in importer {} */
    }
}