mod json;
// mod parquet;
// mod util;

pub struct Batch<T> {
    pub data: Vec<T>,
    pub batch_cursor: usize,
}

pub trait BatchReader<T> {
    fn read_batch(&mut self) -> anyhow::Result<Option<Batch<T>>>;
}

pub trait BatchWriter<T> {
    fn write_batch(&mut self, elements: Vec<T>) -> anyhow::Result<()>;
}
