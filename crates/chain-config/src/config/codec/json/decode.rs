use crate::{
    config::codec::{GroupDecoder, GroupResult},
    in_memory::Decoder as InMemoryDecoder,
    StateConfig,
};

pub struct Decoder<T> {
    in_mem: InMemoryDecoder<StateConfig, T>,
}

impl<T> Decoder<T> {
    pub fn new<R: std::io::Read>(
        mut reader: R,
        group_size: usize,
    ) -> anyhow::Result<Self> {
        // This is a workaround until the Deserialize implementation is fixed to not require a
        // borrowed string over in fuel-vm.
        let mut contents = String::new();
        reader.read_to_string(&mut contents)?;

        let state = serde_json::from_str(&contents)?;
        Ok(Self {
            in_mem: InMemoryDecoder::new(state, group_size),
        })
    }
}

impl<T> Iterator for Decoder<T>
where
    InMemoryDecoder<StateConfig, T>: GroupDecoder<T>,
{
    type Item = GroupResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.in_mem.next()
    }
}

impl<T> GroupDecoder<T> for Decoder<T> where
    InMemoryDecoder<StateConfig, T>: GroupDecoder<T>
{
}
