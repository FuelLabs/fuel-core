use crate::{
    config::{
        codec::GroupEncoder, contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig, StateConfig,
};

#[derive(Default)]
pub struct Encoder {
    sink: StateConfig,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_inner(self) -> StateConfig {
        self.sink
    }
}

impl GroupEncoder for Encoder {
    fn write_coins(&mut self, elements: Vec<CoinConfig>) -> anyhow::Result<()> {
        self.sink.coins.extend(elements);
        Ok(())
    }

    fn write_contracts(&mut self, elements: Vec<ContractConfig>) -> anyhow::Result<()> {
        self.sink.contracts.extend(elements);
        Ok(())
    }

    fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        self.sink.messages.extend(elements);
        Ok(())
    }

    fn write_contract_state(
        &mut self,
        elements: Vec<ContractState>,
    ) -> anyhow::Result<()> {
        self.sink.contract_state.extend(elements);
        Ok(())
    }

    fn write_contract_balance(
        &mut self,
        elements: Vec<ContractBalance>,
    ) -> anyhow::Result<()> {
        self.sink.contract_balance.extend(elements);
        Ok(())
    }

    fn close(self: Box<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}
