use std::ops::RangeInclusive;

#[cfg(test)]
mod test;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct State {
    status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Status {
    Uninitialized,
    Processing(RangeInclusive<u32>),
    Committed(u32),
}

impl State {
    pub fn new(
        committed: impl Into<Option<u32>>,
        observed: impl Into<Option<u32>>,
    ) -> Self {
        let status = match (committed.into(), observed.into()) {
            (Some(committed), Some(observed)) => {
                committed
                    .checked_add(1)
                    .map_or(Status::Committed(committed), |next| {
                        let range = next..=observed;
                        if range.is_empty() {
                            Status::Committed(committed)
                        } else {
                            Status::Processing(range)
                        }
                    })
            }
            (Some(committed), None) => Status::Committed(committed),
            (None, Some(observed)) => Status::Processing(0..=observed),
            (None, None) => Status::Uninitialized,
        };
        Self { status }
    }

    pub fn process_range(&self) -> Option<RangeInclusive<u32>> {
        match &self.status {
            Status::Processing(range) => Some(range.clone()),
            _ => None,
        }
    }

    pub fn commit(&mut self, height: u32) {
        let new_status = match &self.status {
            Status::Processing(range) => match height.cmp(range.end()) {
                std::cmp::Ordering::Less => {
                    Some(Status::Processing(height.saturating_add(1)..=*range.end()))
                }
                std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
                    Some(Status::Committed(height))
                }
            },
            Status::Uninitialized => Some(Status::Committed(height)),
            Status::Committed(existing) => {
                match commit_creates_processing(existing, &height) {
                    Some(range) => Some(Status::Processing(range)),
                    None => Some(Status::Committed(*existing.max(&height))),
                }
            }
        };
        self.apply_status(new_status);
    }

    pub fn observe(&mut self, height: u32) -> bool {
        let new_status = match &self.status {
            Status::Uninitialized => Some(Status::Processing(0..=height)),
            Status::Processing(range) => match range.end().cmp(&height) {
                std::cmp::Ordering::Less => {
                    Some(Status::Processing(*range.start()..=height))
                }
                std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => None,
            },
            Status::Committed(committed) => committed.checked_add(1).and_then(|next| {
                let r = next..=height;
                (!r.is_empty()).then_some(Status::Processing(r))
            }),
        };
        let status_change = new_status.is_some();
        self.apply_status(new_status);
        status_change
    }

    pub fn failed_to_process(&mut self, range: RangeInclusive<u32>) {
        let status = (!range.is_empty())
            .then_some(())
            .and_then(|_| match &self.status {
                Status::Uninitialized | Status::Committed(_) => None,
                Status::Processing(processing) => range
                    .contains(processing.start())
                    .then(|| {
                        processing
                            .start()
                            .checked_sub(1)
                            .map_or(Status::Uninitialized, Status::Committed)
                    })
                    .or_else(|| {
                        (range.contains(processing.end())
                            || processing.contains(range.start()))
                        .then(|| {
                            range
                                .start()
                                .checked_sub(1)
                                .map_or(Status::Uninitialized, |prev| {
                                    Status::Processing(*processing.start()..=prev)
                                })
                        })
                    })
                    .or_else(|| {
                        processing.contains(range.end()).then(|| {
                            processing
                                .start()
                                .checked_sub(1)
                                .map_or(Status::Uninitialized, Status::Committed)
                        })
                    }),
            });
        self.apply_status(status);
    }

    pub fn proposed_height(&self) -> Option<&u32> {
        match &self.status {
            Status::Processing(range) => Some(range.end()),
            _ => None,
        }
    }

    fn apply_status(&mut self, status: Option<Status>) {
        if let Some(s) = status {
            self.status = s;
        }
    }
}

fn commit_creates_processing(
    existing: &u32,
    commit: &u32,
) -> Option<RangeInclusive<u32>> {
    let next = commit.checked_add(1)?;
    let prev_existing = existing.checked_sub(1)?;
    let r = next..=prev_existing;
    (!r.is_empty()).then_some(r)
}
