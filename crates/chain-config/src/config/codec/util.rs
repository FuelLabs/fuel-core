pub(crate) struct InMemorySource {
    // The encoded data inside a `Cursor`. Note this is not our cursor i.e. progress tracker, but
    // rather something rust provides so that you may mimic a file using only a Vec<u8>
    data: Cursor<Vec<u8>>,
    // also has a handy field containing the cursors of all batches encoded in `self.data`. Useful
    // for testing
    element_cursors: Vec<u64>,
}

// impl InMemorySource {
//     pub fn new<T: serde::Serialize>(
//         entries: impl IntoIterator<Item = T>,
//         batch_size: usize,
//     ) -> std::io::Result<Self> {
//         let buffer = Cursor::new(vec![]);
//
//         let writer = TrackingWriter::new(buffer);
//         // this allows us to give up ownership of `writer` but still be able to peek inside it
//         let bytes_written = writer.written_bytes();
//
//         let mut writer = BatchWriter::new(writer);
//         let element_cursors = entries
//             .into_iter()
//             .chunks(batch_size)
//             .into_iter()
//             .map(|chunk| {
//                 // remember the starting position
//                 let cursor = bytes_written.load(std::sync::atomic::Ordering::Relaxed);
//                 writer.write_batch(chunk.collect_vec()).unwrap();
//                 // since `GenericWriter` has a buffered writer inside of it, it won't flush all the
//                 // time. This is bad for us here since we want all the data flushed to our
//                 // `TrackingWriter` so that it may count the bytes. We use that count to provide
//                 // the cursors for each batch -- useful for testing.
//                 writer.flush().unwrap();
//                 cursor
//             })
//             .collect();
//
//         Ok(Self {
//             // basically unpeals the writers, first we get the tracking writer, then we get the
//             // Cursor we gave it. into_inner will flush so we can be sure that the final Cursor has
//             // all the data. Also we did a bunch of flushing above
//             data: writer.into_inner()?.into_inner(),
//             element_cursors,
//         })
//     }
//
//     // useful for tests so we don't have to hardcode boundaries
//     pub fn batch_cursors(&self) -> &[u64] {
//         &self.element_cursors
//     }
// }

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
