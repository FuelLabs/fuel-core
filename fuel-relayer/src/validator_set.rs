use std::collections::HashMap;

use fuel_core_interfaces::relayer::RelayerDb;
use fuel_tx::Address;
use tracing::warn;

#[derive(Default)]
pub struct CurrentValidatorSet {
    /// Current validator set
    pub set: HashMap<Address, u64>,
    /// Current DA block
    pub da_height: u64,
}

impl CurrentValidatorSet {
    // probably not going to metter a lot we expect for validator stake to be mostly unchanged.
    // TODO if it takes a lot of time to load it is good to optimize.
    pub async fn load_get_validators(&mut self, db: &dyn RelayerDb) {
        self.da_height = db.get_validators_da_height().await;
        self.set = db.get_validators().await;
    }

    pub fn get_validator_set(&mut self, block_number: u64) -> Option<HashMap<Address, u64>> {
        if self.da_height == block_number {
            return Some(self.set.clone());
        }
        None
    }

    /// new_block_diff is finality slider adjusted
    /// it supports only going up
    pub async fn bump_validators_to_da_height(&mut self, da_height: u64, db: &mut dyn RelayerDb) {
        match self.da_height.cmp(&da_height) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => {
                // unusual but do nothing
                return;
            }
            std::cmp::Ordering::Greater => {
                // if curent block is greater then new included block there is some problem.
                warn!(
                    "curent height {:?} is greater then new height {:?} there is some problem",
                    self.da_height, da_height
                );
                return;
            }
        }

        let diffs = db
            .get_validator_diffs(self.da_height, Some(da_height))
            .await;
        let diffs = diffs.into_iter().fold(HashMap::new(), |mut g, (_, diff)| {
            g.extend(diff.into_iter());
            g
        });
        // apply diffs to validators inside db
        db.apply_validator_diffs(&diffs, da_height).await;
        // apply diffs this set
        self.set.extend(diffs);
        self.da_height = da_height;
    }
}
