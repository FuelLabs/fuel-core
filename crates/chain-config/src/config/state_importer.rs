use std::marker::PhantomData;

use fuel_core_types::fuel_types::Bytes32;

use crate::{
    CoinConfig,
    ContractConfig,
    MessageConfig,
};

pub struct StateImporter<T> {
    phantom_data: PhantomData<T>,
}

pub enum ContractComponent {
    ContractMetadata(ContractConfig),
    ContractState(Bytes32, Bytes32),
    ContractAsset(Bytes32, usize),
}

impl<T> StateImporter<T> {
    pub fn next(&mut self) -> Option<anyhow::Result<T>> {
        todo!()
    }

    pub fn current_cursor(&self) -> usize {
        todo!()
    }
}

impl StateImporter<CoinConfig> {
    pub fn new(_reader: impl std::io::Read) -> Self {
        todo!()
    }

    pub fn local_testnet() -> Self {
        todo!()
    }

    pub fn messages(self) -> StateImporter<MessageConfig> {
        StateImporter::<MessageConfig> {
            phantom_data: PhantomData,
        }
    }
}

impl StateImporter<MessageConfig> {
    pub fn contracts(self) -> StateImporter<ContractComponent> {
        StateImporter::<ContractComponent> {
            phantom_data: PhantomData,
        }
    }
}

impl<T> Iterator for StateImporter<T> {
    type Item = anyhow::Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CoinConfig,
        StateImporter,
    };

    #[test]
    fn test() {
        let mut importer = StateImporter::<CoinConfig> {
            phantom_data: std::marker::PhantomData,
        };

        let next_item = importer.next();
        let cursor = importer.current_cursor();

        for coin in importer {}

        let mut importer = importer.messages();

        let next_item = importer.next();
        let cursor = importer.current_cursor();

        for message in importer {}

        let mut importer = importer.contracts();

        let next_item = importer.next();
        let cursor = importer.current_cursor();

        for contract in importer {}
    }
}
