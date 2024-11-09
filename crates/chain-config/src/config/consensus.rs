use alloc::collections::BTreeMap;
use fuel_core_types::{
    fuel_tx::Input,
    fuel_types::{
        Address,
        BlockHeight,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

use crate as fuel_core_chain_config;
use fuel_core_chain_config::default_consensus_dev_key;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum ConsensusConfig {
    PoA { signing_key: Address },
    PoAV2(PoAV2),
}

impl ConsensusConfig {
    pub fn default_poa() -> Self {
        ConsensusConfig::PoAV2(PoAV2 {
            genesis_signing_key: Input::owner(&default_consensus_dev_key().public_key()),
            signing_key_overrides: Default::default(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct PoAV2 {
    genesis_signing_key: Address,
    signing_key_overrides: BTreeMap<BlockHeight, Address>,
}

impl PoAV2 {
    pub fn new(
        genesis_signing_key: Address,
        signing_key_overrides: BTreeMap<BlockHeight, Address>,
    ) -> Self {
        PoAV2 {
            genesis_signing_key,
            signing_key_overrides,
        }
    }

    /// Returns the signing key for the given block height.
    pub fn signing_key_at(&self, height: BlockHeight) -> Address {
        if self.signing_key_overrides.is_empty() {
            self.genesis_signing_key
        } else {
            self.signing_key_overrides
                .range(..=height)
                .last()
                .map(|(_, key)| key)
                .cloned()
                .unwrap_or(self.genesis_signing_key)
        }
    }

    /// Returns overrides for all the signing keys.
    pub fn get_all_overrides(&self) -> &BTreeMap<BlockHeight, Address> {
        &self.signing_key_overrides
    }

    #[cfg(feature = "test-helpers")]
    pub fn set_genesis_signing_key(&mut self, key: Address) {
        self.genesis_signing_key = key;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_key_at_works() {
        // Given
        let genesis_signing_key = Address::from([1; 32]);
        let signing_key_after_10 = Address::from([2; 32]);
        let signing_key_after_20 = Address::from([3; 32]);
        let signing_key_after_30 = Address::from([4; 32]);
        let signing_key_overrides = vec![
            (10u32.into(), signing_key_after_10),
            (20u32.into(), signing_key_after_20),
            (30u32.into(), signing_key_after_30),
        ]
        .into_iter()
        .collect();
        let poa = PoAV2 {
            genesis_signing_key,
            signing_key_overrides,
        };

        // When/Then
        assert_eq!(poa.signing_key_at(0u32.into()), genesis_signing_key);
        assert_eq!(poa.signing_key_at(9u32.into()), genesis_signing_key);
        assert_eq!(poa.signing_key_at(10u32.into()), signing_key_after_10);
        assert_eq!(poa.signing_key_at(19u32.into()), signing_key_after_10);
        assert_eq!(poa.signing_key_at(20u32.into()), signing_key_after_20);
        assert_eq!(poa.signing_key_at(29u32.into()), signing_key_after_20);
        assert_eq!(poa.signing_key_at(30u32.into()), signing_key_after_30);
        assert_eq!(poa.signing_key_at(40u32.into()), signing_key_after_30);
        assert_eq!(
            poa.signing_key_at(4_000_000u32.into()),
            signing_key_after_30
        );
    }
}
