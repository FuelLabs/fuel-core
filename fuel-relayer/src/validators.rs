use fuel_core_interfaces::{
    model::{DaBlockHeight, ValidatorStake},
    relayer::RelayerDb,
};
use fuel_tx::Address;
use std::collections::{hash_map::Entry, HashMap};
use tracing::{error, info};

/// It contains list of Validators and its stake and consensus public key.
/// We dont expect big number of validators in that sense we are okey to have it all in memory
/// for fast access. Data availability height (da_height) represent snapshot height that set represent.
/// Validator Set is same as it is inside database.
#[derive(Default)]
pub struct Validators {
    /// Validator set
    pub set: HashMap<Address, (ValidatorStake, Option<Address>)>,
    /// Da height
    pub da_height: DaBlockHeight,
}

impl Validators {
    // probably not going to metter a lot we expect for validator stake to be mostly unchanged.
    // TODO if it takes a lot of time to load it is good to optimize.
    pub async fn load(&mut self, db: &dyn RelayerDb) {
        self.da_height = db.get_validators_da_height().await;
        self.set = db.get_validators().await;
    }

    /// Get validator set
    pub async fn get(
        &mut self,
        da_height: DaBlockHeight,
    ) -> Option<HashMap<Address, (u64, Option<Address>)>> {
        // TODO apply down drift https://github.com/FuelLabs/fuel-core/issues/365
        if self.da_height == da_height {
            return Some(self.set.clone());
        }
        None
    }

    /// Bump validator set to new high da_height.
    /// Iterate over database StakingDiff column and apply them to our current `self.set`.
    pub async fn bump_set_to_da_height(
        &mut self,
        da_height: DaBlockHeight,
        db: &mut dyn RelayerDb,
    ) {
        match self.da_height.cmp(&da_height) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => {
                // unusual but do nothing
                info!("Already on same validator set height {da_height}");
                return;
            }
            std::cmp::Ordering::Greater => {
                // happens when initiating watch for ganache new blocks, it buffers few old blocks and sends them over.
                error!(
                    "current height {} is greater then new height {da_height}",
                    self.da_height
                );
                return;
            }
        }

        let mut validators = HashMap::new();
        // get staking diffs.
        let diffs = db
            .get_staking_diffs(self.da_height + 1, Some(da_height))
            .await;
        let mut delegates_cached: HashMap<Address, Option<HashMap<Address, u64>>> = HashMap::new();
        for (diff_height, diff) in diffs.into_iter() {
            // update consensus_key
            for (validator, consensus_key) in diff.validators {
                validators
                    .entry(validator)
                    .or_insert_with(|| self.set.get(&validator).cloned().unwrap_or_default())
                    .1 = consensus_key;
            }

            // for every delegates, cache it and if it is not in cache query db's delegates_index for earlier delegate set.
            for (delegator, delegation) in diff.delegations.into_iter() {
                // add new delegation stake.
                if let Some(ref delegation) = delegation {
                    for (validator, stake) in delegation {
                        validators
                            .entry(*validator)
                            .or_insert_with(|| self.set.get(validator).cloned().unwrap_or_default())
                            // increate stake
                            .0 += stake;
                    }
                }

                // get old delegation
                let old_delegation = match delegates_cached.entry(delegator) {
                    Entry::Vacant(entry) => {
                        let old_delegation = db.get_last_delegation(&delegator, diff_height).await;
                        entry.insert(delegation);
                        old_delegation
                    }
                    Entry::Occupied(ref mut entry) => {
                        let old_delegation = entry.get_mut();
                        let ret = std::mem::take(old_delegation);
                        *old_delegation = delegation;
                        ret
                    }
                };

                // deduce old delegation stake if exist
                if let Some(old_delegations) = old_delegation {
                    for (validator, old_stake) in old_delegations.into_iter() {
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
