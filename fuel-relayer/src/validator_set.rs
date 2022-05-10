use std::collections::{hash_map::Entry, HashMap};

use fuel_core_interfaces::{
    model::{DaBlockHeight, ValidatorStake},
    relayer::RelayerDb,
};
use fuel_tx::Address;
use tracing::{error, info};

/// It contains list of Validators and its stake and consensus public key.
/// We dont expect big number of validators in that sense we are okey to have it all in memory
/// Data availability height (da_height) represent snapshot in moment when set is seen.
#[derive(Default)]
pub struct CurrentValidatorSet {
    /// Current validator set
    pub set: HashMap<Address, (ValidatorStake, Option<Address>)>,
    /// current fuel block
    pub da_height: u64,
}

impl CurrentValidatorSet {
    // probably not going to metter a lot we expect for validator stake to be mostly unchanged.
    // TODO if it takes a lot of time to load it is good to optimize.
    pub async fn load_get_validators(&mut self, db: &dyn RelayerDb) {
        self.da_height = db.get_validators_da_height().await;
        self.set = db.get_validators().await;
    }

    pub fn get_validator_set(
        &mut self,
        da_height: u64,
    ) -> Option<HashMap<Address, (u64, Option<Address>)>> {
        // TODO apply down drift
        if self.da_height == da_height {
            return Some(self.set.clone());
        }
        None
    }

    /// new_block_diff is finality slider adjusted
    /// it supports only going up
    pub async fn bump_validators_to_da_height(
        &mut self,
        da_height: DaBlockHeight,
        db: &mut dyn RelayerDb,
    ) {
        match self.da_height.cmp(&da_height) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => {
                // unusual but do nothing
                info!("Already on same validator set height");
                return;
            }
            std::cmp::Ordering::Greater => {
                // if curent block is greater then new included block there is some problem.
                error!(
                    "curent height {:?} is greater then new height {:?} there is some problem",
                    self.da_height, da_height
                );
                return;
            }
        }

        let mut validators: HashMap<Address, (ValidatorStake, Option<Address>)> = HashMap::new();
        // get staking diffs.
        let diffs = db.get_staking_diff(self.da_height, Some(da_height)).await;
        let mut delegates_cache: HashMap<Address, Option<HashMap<Address, u64>>> = HashMap::new();
        for (diff_height, diff) in diffs {
            // update consensus_key
            for (validator, consensus_key) in diff.validators {
                validators
                    .entry(validator)
                    .or_insert_with(|| self.set.get(&validator).cloned().unwrap_or_default())
                    .1 = consensus_key;
            }

            // for every delegates, cache it and if it is not in cache query db's delegates_index for earlier delegate set.
            for (delegator, delegation) in diff.delegations.iter() {
                // add new delegation stake.
                if let Some(delegation) = delegation {
                    for (validator, stake) in delegation {
                        validators
                            .entry(*validator)
                            .or_insert_with(|| self.set.get(validator).cloned().unwrap_or_default())
                            // increate stake
                            .0 += *stake;
                    }
                }

                // get old delegation
                let old_delegation = match delegates_cache.entry(*delegator) {
                    Entry::Vacant(entry) => {
                        let old_delegation = db.get_last_delegation(delegator, diff_height).await;
                        entry.insert(delegation.clone());
                        old_delegation
                    }
                    Entry::Occupied(ref mut entry) => {
                        let old_delegation = entry.get_mut();
                        let ret = std::mem::take(old_delegation);
                        *old_delegation = delegation.clone();
                        ret
                    }
                };

                // remove old delegation stake if exist
                if let Some(old_delegations) = old_delegation {
                    // remove old delegations
                    for (validator, old_stake) in old_delegations.into_iter() {
                        // it is okey to cast u64 stake to i64, stake is not going to be that big.
                        validators
                            .entry(validator)
                            .or_insert_with(|| {
                                self.set.get(&validator).cloned().unwrap_or_default()
                            })
                            // decrease undelegated stake
                            .0 -= old_stake;
                    }
                }
            }
        }
        // apply diffs to validators inside db
        db.apply_validator_diffs(da_height, &validators).await;
        // apply diffs this set
        self.set.extend(validators);
        self.da_height = da_height;
    }
}
