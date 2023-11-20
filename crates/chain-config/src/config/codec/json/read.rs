use crate::{
    config::codec::{GroupDecoder, GroupResult},
    Decoder, StateConfig,
};

pub struct JsonDecoder<T> {
    in_mem: Decoder<StateConfig, T>,
}

impl<T> JsonDecoder<T> {
    pub fn new<R: std::io::Read>(
        mut reader: R,
        batch_size: usize,
    ) -> anyhow::Result<Self> {
        // This is a workaround until the Deserialize implementation is fixed to not require a
        // borrowed string over in fuel-vm.
        let mut contents = String::new();
        reader.read_to_string(&mut contents)?;

        let state = serde_json::from_str(&contents)?;
        Ok(Self {
            in_mem: Decoder::new(state, batch_size),
        })
    }
}

impl<T> Iterator for JsonDecoder<T>
where
    Decoder<StateConfig, T>: GroupDecoder<T>,
{
    type Item = GroupResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.in_mem.next()
    }
}

impl<T> GroupDecoder<T> for JsonDecoder<T> where Decoder<StateConfig, T>: GroupDecoder<T> {}
