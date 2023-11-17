use std::io::Write;

use crate::{
    config::{
        codec::BatchWriterTrait, contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig, ContractConfig, MessageConfig,
};

use super::chain_state::ChainState;

pub struct JsonBatchWriter<W> {
    sink: W,
    temp_storage: ChainState,
}

impl<W> JsonBatchWriter<W> {
    pub(crate) fn new(sink: W) -> Self {
        Self {
            temp_storage: ChainState {
                coins: vec![],
                messages: vec![],
                contracts: vec![],
                contract_state: vec![],
                contract_balance: vec![],
            },
            sink,
        }
    }
}

impl<W: Write> BatchWriterTrait for JsonBatchWriter<W> {
    fn write_coins(&mut self, elements: Vec<CoinConfig>) -> anyhow::Result<()> {
        self.temp_storage.coins.extend(elements);
        Ok(())
    }

    fn write_contracts(&mut self, elements: Vec<ContractConfig>) -> anyhow::Result<()> {
        self.temp_storage.contracts.extend(elements);
        Ok(())
    }

    fn write_messages(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        self.temp_storage.messages.extend(elements);
        Ok(())
    }

    fn write_contract_state(
        &mut self,
        elements: Vec<ContractState>,
    ) -> anyhow::Result<()> {
        self.temp_storage.contract_state.extend(elements);
        Ok(())
    }

    fn write_contract_balance(
        &mut self,
        elements: Vec<ContractBalance>,
    ) -> anyhow::Result<()> {
        self.temp_storage.contract_balance.extend(elements);
        Ok(())
    }

    fn close(mut self: Box<Self>) -> anyhow::Result<()> {
        serde_json::to_writer(&mut self.sink, &self.temp_storage)?;
        self.sink.flush()?;
        Ok(())
    }
}
