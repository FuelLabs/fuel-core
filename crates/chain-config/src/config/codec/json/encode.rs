use std::io::Write;

use crate::{
    config::{
        codec::StateEncoder,
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig,
    ContractConfig,
    MessageConfig,
    StateConfig,
};

use crate::in_memory::Encoder as InMemoryEncoder;

pub struct Encoder<W> {
    sink: W,
    temp_storage: InMemoryEncoder<StateConfig>,
}

impl<W> Encoder<W> {
    pub fn new(sink: W) -> Self {
        Self {
            sink,
            temp_storage: InMemoryEncoder::new(StateConfig::default()),
        }
    }
}

impl<W: Write> StateEncoder for Encoder<W> {
    fn write_coins(&mut self, elements: Vec<CoinConfig>) -> anyhow::Result<()> {
        self.temp_storage.write_coins(elements)
    }

    fn write_contracts(&mut self, elements: Vec<ContractConfig>) -> anyhow::Result<()> {
        self.temp_storage.write_contracts(elements)
    }

    fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        self.temp_storage.write_messages(elements)
    }

    fn write_contract_state(
        &mut self,
        elements: Vec<ContractState>,
    ) -> anyhow::Result<()> {
        self.temp_storage.write_contract_state(elements)
    }

    fn write_contract_balance(
        &mut self,
        elements: Vec<ContractBalance>,
    ) -> anyhow::Result<()> {
        self.temp_storage.write_contract_balance(elements)
    }

    fn close(mut self: Box<Self>) -> anyhow::Result<()> {
        let state_config = self.temp_storage.into_inner();
        serde_json::to_writer(&mut self.sink, &state_config)?;
        self.sink.flush()?;
        Ok(())
    }
}
