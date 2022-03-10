use std::collections::HashMap;

use fuel_core_interfaces::relayer::RelayerDb;
use fuel_tx::Address;

pub struct CurrentValidatorSet {
    /// Current validator set
    pub set: HashMap<Address, u64>,
    /// current fuel block
    pub eth_height: u64,
}

impl Default for CurrentValidatorSet {
    fn default() -> Self {
        Self::new()
    }
}

impl CurrentValidatorSet {
    pub fn new() -> Self {
        Self {
            set: HashMap::new(),
            eth_height: 0,
        }
    }

    // probably not going to metter a lot we expect for validator stake to be mostly unchanged.
    // TODO in becomes troublesome to load and takes a lot of time, it is good to optimize
    pub async fn load_current_validator_set(&mut self, db: &dyn RelayerDb) {
        self.eth_height = db.get_current_validator_set_eth_height().await;
        self.set = db.current_validator_set().await;
    }

    pub fn get_validator_set(&mut self, block_number: u64) -> Option<HashMap<Address, u64>> {
        if self.eth_height == block_number {
            return Some(self.set.clone());
        }
        None
    }

    /// new_block_diff is finality slider adjusted
    /// it supports only going up
    pub async fn bump_set_to_eth_height(&mut self, eth_height: u64, db: &dyn RelayerDb) {
        match self.eth_height.cmp(&eth_height) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => {
                // unusual but do nothing
                return;
            }
            std::cmp::Ordering::Greater => {
                // if curent block is greater then new included block there is some problem.
                return;
            }
        }

        let diffs = db
            .get_validator_set_diff(self.eth_height, Some(eth_height))
            .await;
        for (_, diff) in diffs {
            for (address, new_value) in diff {
                self.set.insert(address, new_value);
            }
        }
        self.eth_height = eth_height;
    }
}
