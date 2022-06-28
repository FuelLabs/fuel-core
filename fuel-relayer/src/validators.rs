use fuel_core_interfaces::{
    common::fuel_tx::Address,
    model::{ConsensusId, DaBlockHeight, ValidatorId, ValidatorStake},
    relayer::{RelayerDb, ValidatorDiff},
};
use std::collections::{hash_map::Entry, HashMap};
use tracing::{debug, error, info};

/// It contains list of Validators and its stake and consensus public key.
/// We dont expect big number of validators in that sense we are okey to have it all in memory
/// for fast access. Data availability height (da_height) represent snapshot height that set represent.
/// Validator Set is same as it is inside database.
#[derive(Default)]
pub struct Validators {
    /// Validator set
    pub set: HashMap<ValidatorId, (ValidatorStake, Option<ConsensusId>)>,
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

    /// Get validator set for da_height.
    pub async fn get(
        &self,
        da_height: DaBlockHeight,
        db: &mut dyn RelayerDb,
    ) -> Option<HashMap<ValidatorId, (u64, Option<ConsensusId>)>> {
        match self.da_height.cmp(&da_height) {
            std::cmp::Ordering::Less => {
                // We request validator set that we still didnt finalized or know about.
                // Probably there is eth client sync problem
                error!(
                    "current height {} is less then requested validator set height: {da_height}",
                    self.da_height
                );
                None
            }
            std::cmp::Ordering::Equal => {
                // unusual but do nothing
                debug!("Get last finalized set height: {da_height}");
                let set = self
                    .set
                    .iter()
                    .filter(|(_, (stake, consensus_key))| *stake != 0 && consensus_key.is_some())
                    .map(|(k, v)| (*k, *v))
                    .collect();
                Some(set)
            }
            std::cmp::Ordering::Greater => {
                // slightly drift to past

                let mut validators = self.set.clone();
                // get staking diffs to revert from our current set.
                let diffs = db
                    .get_staking_diffs(da_height + 1, Some(self.da_height))
                    .await;

                for (diff_height, diff) in diffs.into_iter().rev() {
                    // update consensus_key
                    for (
                        validator,
                        ValidatorDiff {
                            previous_consensus_key,
                            ..
                        },
                    ) in diff.validators
                    {
                        if let Some((_, consensus_key)) = validators.get_mut(&validator) {
                            *consensus_key = previous_consensus_key;
                        } else {
                            panic!("Validator should be present when reverting diff");
                        }
                    }

                    // for every delegates, cache it and if it is not in cache query db's delegates_index for earlier delegate set.
                    for (delegator, delegation) in diff.delegations {
                        // add new delegation stake.
                        if let Some(ref delegation) = delegation {
                            for (validator, stake) in delegation {
                                validators
                                    .entry(*validator)
                                    .or_insert_with(|| {
                                        self.set.get(validator).cloned().unwrap_or_default()
                                    })
                                    // remove stake
                                    .0 -= stake;
                            }
                        }

                        // get older delegation to add again to validator stake
                        if let Some(delegations) = db
                            .get_first_lesser_delegation(&delegator, diff_height)
                            .await
                        {
                            for (validator, stake) in delegations {
                                validators
                                    .entry(validator)
                                    .or_insert_with(|| {
                                        self.set.get(&validator).cloned().unwrap_or_default()
                                    })
                                    // remove stake
                                    .0 += stake;
                            }
                        };
                    }
                }

                validators
                    .retain(|_, (stake, consensus_key)| *stake != 0 && consensus_key.is_some());
                Some(validators)
            }
        }
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
        let mut delegates_cached: HashMap<Address, Option<HashMap<ValidatorId, ValidatorStake>>> =
            HashMap::new();
        for (diff_height, diff) in diffs.into_iter() {
            // update consensus_key
            for (
                validator,
                ValidatorDiff {
                    new_consensus_key, ..
                },
            ) in diff.validators
            {
                validators
                    .entry(validator)
                    .or_insert_with(|| self.set.get(&validator).cloned().unwrap_or_default())
                    .1 = new_consensus_key;
            }

            // for every delegates, cache it and if it is not in cache query db's delegates_index for earlier delegate set.
            for (delegator, delegation) in diff.delegations {
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
                        let old_delegation = db
                            .get_first_lesser_delegation(&delegator, diff_height)
                            .await;
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
                                self.set
                                    .get(&validator)
                                    .cloned()
                                    .expect("Expect for validator to exists")
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

// testing for this mod is done inside finalization_queue.rs mod.
