use crate::database::{
    database_description::{
        off_chain::OffChain,
        on_chain::OnChain,
        relayer::Relayer,
    },
    Database,
    Result as DatabaseResult,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};

/// A database that combines the on-chain, off-chain and relayer databases into one entity.
#[derive(Default, Clone)]
pub struct CombinedDatabase {
    on_chain: Database<OnChain>,
    off_chain: Database<OffChain>,
    relayer: Database<Relayer>,
}

impl CombinedDatabase {
    pub fn new(
        on_chain: Database<OnChain>,
        off_chain: Database<OffChain>,
        relayer: Database<Relayer>,
    ) -> Self {
        Self {
            on_chain,
            off_chain,
            relayer,
        }
    }

    #[cfg(feature = "rocksdb")]
    pub fn open(path: &std::path::Path, capacity: usize) -> DatabaseResult<Self> {
        // TODO: Use different cache sizes for different databases
        let on_chain = Database::open(path, capacity)?;
        let off_chain = Database::open(path, capacity)?;
        let relayer = Database::open(path, capacity)?;
        Ok(Self {
            on_chain,
            off_chain,
            relayer,
        })
    }

    pub fn in_memory() -> Self {
        Self::new(
            Database::in_memory(),
            Database::in_memory(),
            Database::in_memory(),
        )
    }

    pub fn init(
        &mut self,
        block_height: &BlockHeight,
        da_block_height: &DaBlockHeight,
    ) -> StorageResult<()> {
        self.on_chain.init(block_height)?;
        self.off_chain.init(block_height)?;
        self.relayer.init(da_block_height)?;
        Ok(())
    }

    pub fn on_chain(&self) -> &Database<OnChain> {
        &self.on_chain
    }

    #[cfg(any(feature = "test-helpers", test))]
    pub fn on_chain_mut(&mut self) -> &mut Database<OnChain> {
        &mut self.on_chain
    }

    pub fn off_chain(&self) -> &Database<OffChain> {
        &self.off_chain
    }

    #[cfg(any(feature = "test-helpers", test))]
    pub fn off_chain_mut(&mut self) -> &mut Database<OffChain> {
        &mut self.off_chain
    }

    pub fn relayer(&self) -> &Database<Relayer> {
        &self.relayer
    }

    pub fn flush(self) -> DatabaseResult<()> {
        self.on_chain.flush()?;
        self.off_chain.flush()?;
        self.relayer.flush()?;
        Ok(())
    }
}
