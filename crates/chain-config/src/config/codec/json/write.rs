use crate::{
    config::{
        codec::BatchWriter,
        contract_balance::ContractBalance,
        contract_state::ContractState,
    },
    CoinConfig,
    ContractConfig,
    MessageConfig,
};

use super::chain_state::ChainState;

pub(crate) struct JsonBatchWriter {
    source: ChainState,
}

impl JsonBatchWriter {
    pub(crate) fn new() -> Self {
        Self {
            source: ChainState {
                coins: vec![],
                messages: vec![],
                contracts: vec![],
                contract_state: vec![],
                contract_balance: vec![],
            },
        }
    }

    pub(crate) fn state(&self) -> &ChainState {
        &self.source
    }
}

impl BatchWriter<ContractConfig> for JsonBatchWriter {
    fn write_batch(&mut self, elements: Vec<ContractConfig>) -> anyhow::Result<()> {
        self.source.contracts.extend(elements);
        Ok(())
    }
}
impl BatchWriter<MessageConfig> for JsonBatchWriter {
    fn write_batch(&mut self, elements: Vec<MessageConfig>) -> anyhow::Result<()> {
        self.source.messages.extend(elements);
        Ok(())
    }
}
impl BatchWriter<CoinConfig> for JsonBatchWriter {
    fn write_batch(&mut self, elements: Vec<CoinConfig>) -> anyhow::Result<()> {
        self.source.coins.extend(elements);
        Ok(())
    }
}
impl BatchWriter<ContractState> for JsonBatchWriter {
    fn write_batch(&mut self, elements: Vec<ContractState>) -> anyhow::Result<()> {
        self.source.contract_state.push(elements);
        Ok(())
    }
}
impl BatchWriter<ContractBalance> for JsonBatchWriter {
    fn write_batch(&mut self, elements: Vec<ContractBalance>) -> anyhow::Result<()> {
        self.source.contract_balance.push(elements);
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[cfg(feature = "random")]
    #[test]
    fn writes_correctly() {
        let data = ChainState::random(100, 100, &mut rand::thread_rng());
        let mut writer = JsonBatchWriter::new();

        writer.write_batch(data.contracts.clone()).unwrap();
        writer.write_batch(data.coins.clone()).unwrap();
        writer.write_batch(data.messages.clone()).unwrap();
        for batch in data.contract_state.clone() {
            writer.write_batch(batch).unwrap();
        }
        for batch in data.contract_balance.clone() {
            writer.write_batch(batch).unwrap();
        }

        assert_eq!(writer.state(), &data);
    }
}
