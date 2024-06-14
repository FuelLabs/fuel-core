pub mod decode;
pub mod encode;

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use itertools::Itertools;
    use parquet::{
        basic::ZstdLevel,
        file::reader::{
            ChunkReader,
            Length,
        },
    };
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };
    use std::{
        io::{
            Cursor,
            Read,
            Seek,
            SeekFrom,
        },
        iter::repeat_with,
        sync::{
            atomic::AtomicU64,
            Arc,
            Mutex,
        },
    };

    use super::{
        decode::*,
        encode::*,
    };

    #[derive(Clone)]
    struct TrackingReader {
        source: Arc<Mutex<Cursor<Vec<u8>>>>,
        read_bytes: Arc<AtomicU64>,
    }

    impl TrackingReader {
        fn new(source: Vec<u8>) -> Self {
            Self {
                source: Arc::new(Mutex::new(Cursor::new(source))),
                read_bytes: Default::default(),
            }
        }
    }

    impl Seek for TrackingReader {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.source.lock().unwrap().seek(pos)
        }
    }

    impl Read for TrackingReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let amount = self.source.lock().unwrap().read(buf)?;
            self.read_bytes
                .fetch_add(amount as u64, std::sync::atomic::Ordering::SeqCst);
            Ok(amount)
        }
    }

    impl Length for TrackingReader {
        fn len(&self) -> u64 {
            let mut source = self.source.lock().unwrap();

            let current = source.stream_position().unwrap();

            source.seek(SeekFrom::Start(0)).unwrap();
            let len = source.seek(SeekFrom::End(0)).unwrap();

            source.seek(SeekFrom::Start(current)).unwrap();
            len
        }
    }

    impl ChunkReader for TrackingReader {
        type T = TrackingReader;

        fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
            let mut new_reader = self.clone();
            new_reader.seek(SeekFrom::Start(start))?;

            Ok(new_reader)
        }

        fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
            let mut buf = Vec::with_capacity(length);
            let reader = self.get_read(start)?;
            reader.take(length as u64).read_to_end(&mut buf)?;

            Ok(Bytes::from(buf))
        }
    }

    #[test]
    fn can_skip_groups_without_reading_whole_file() {
        // given
        let mut buffer = vec![];
        let mut encoder = Encoder::new(
            &mut buffer,
            parquet::basic::Compression::ZSTD(ZstdLevel::try_new(1).unwrap()),
        )
        .unwrap();
        let mut rng = StdRng::seed_from_u64(0);

        let big_group = repeat_with(|| rng.gen::<[u8; 32]>().to_vec())
            .take(1000)
            .collect_vec();
        encoder.write(big_group).unwrap();

        let small_group = vec![rng.gen::<[u8; 32]>().to_vec()];
        encoder.write(small_group).unwrap();
        encoder.close().unwrap();
        let total_size = buffer.len();

        let bytes = TrackingReader::new(buffer);
        let bytes_read = bytes.read_bytes.clone();

        let mut decoder = Decoder::new(bytes).unwrap();

        // when
        let _: Vec<_> = decoder.nth(1).unwrap().unwrap();

        // then
        let actually_read = bytes_read.load(std::sync::atomic::Ordering::SeqCst);

        assert_eq!(total_size, 36930);
        assert_eq!(actually_read, 509);
    }
}
