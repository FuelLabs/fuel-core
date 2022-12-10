use fuel_core_interfaces::{
    common::prelude::{
        Hasher,
        MerkleRoot,
    },
    model::Genesis,
};

pub trait GenesisCommitment {
    /// Calculates the merkle root of the state of the entity.
    fn root(&mut self) -> anyhow::Result<MerkleRoot>;
}

impl GenesisCommitment for Genesis {
    fn root(&mut self) -> anyhow::Result<MerkleRoot> {
        let genesis_hash = *Hasher::default()
            .chain(self.chain_config_hash)
            .chain(self.coins_root)
            .chain(self.contracts_root)
            .chain(self.messages_root)
            .finalize();

        Ok(genesis_hash)
    }
}
