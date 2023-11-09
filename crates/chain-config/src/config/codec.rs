mod json;
mod parquet;
// mod util;

pub struct Batch<T> {
    pub data: Vec<T>,
    pub group_index: usize,
}

pub trait BatchReader<T, I: IntoIterator<Item = anyhow::Result<Batch<T>>>> {
    fn batch_iter(self) -> I;
}

pub trait BatchWriter<T> {
    fn write_batch(&mut self, elements: Vec<T>) -> anyhow::Result<()>;
}
