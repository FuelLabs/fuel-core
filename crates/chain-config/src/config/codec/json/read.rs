use crate::{
    config::codec::{GroupDecoder, GroupResult},
    Decoder,
};

pub struct JsonDecoder<T> {
    in_mem: Decoder<T>,
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

impl<T> GroupDecoder for JsonDecoder<T>
where
    Decoder<T>: GroupDecoder<GroupItem = T>,
{
    type GroupItem = T;

    fn next_group(&mut self) -> Option<GroupResult<Self::GroupItem>> {
        self.in_mem.next_group()
    }

    fn nth_group(&mut self, n: usize) -> Option<GroupResult<Self::GroupItem>> {
        self.in_mem.nth_group(n)
    }
}
