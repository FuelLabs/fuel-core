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
            (Some(committed), Some(observed)) => match committed.checked_add(1) {
                Some(next) => {
                    let range = next..=observed;
                    if range.is_empty() {
                        Status::Committed(committed)
                    } else {
                        Status::Processing(range)
                    }
                }
                None => Status::Committed(committed),
            },
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
        match &self.status {
            Status::Processing(range) => {
                if height >= *range.end() {
                    self.status = Status::Committed(height);
                } else {
                    self.status =
                        Status::Processing(height.saturating_add(1)..=*range.end())
                }
            }
            Status::Uninitialized => {
                self.status = Status::Committed(height);
            }
            Status::Committed(existing) => {
                match creates_new_processing(existing, &height) {
                    Some(range) => {
                        self.status = Status::Processing(range);
                    }
                    None => {
                        self.status = Status::Committed(*existing.max(&height));
                    }
                }
            }
        }
    }

    pub fn observe(&mut self, height: u32) -> bool {
        let mut status_change = true;
        match &self.status {
            Status::Uninitialized => {
                self.status = Status::Processing(0..=height);
            }
            Status::Processing(range) if *range.end() < height => {
                self.status = Status::Processing(*range.start()..=height);
            }
            Status::Committed(committed) => {
                if let Some(next) = committed.checked_add(1) {
                    let r = next..=height;
                    if !r.is_empty() {
                        self.status = Status::Processing(r);
                    }
                }
            }
            _ => {
                status_change = false;
            }
        }
        status_change
    }

    pub fn failed_to_process(&mut self, range: RangeInclusive<u32>) {
        if !range.is_empty() {
            match &self.status {
                Status::Uninitialized => (),
                Status::Processing(processing) => {
                    if range.contains(processing.start()) {
                        match processing.start().checked_sub(1) {
                            Some(prev) => {
                                self.status = Status::Committed(prev);
                            }
                            None => {
                                self.status = Status::Uninitialized;
                            }
                        }
                    } else if range.contains(processing.end())
                        || processing.contains(range.start())
                    {
                        match range.start().checked_sub(1) {
                            Some(prev) => {
                                self.status =
                                    Status::Processing(*processing.start()..=prev);
                            }
                            None => {
                                self.status = Status::Uninitialized;
                            }
                        }
                    } else if processing.contains(range.end()) {
                        match processing.start().checked_sub(1) {
                            Some(prev) => {
                                self.status = Status::Committed(prev);
                            }
                            None => {
                                self.status = Status::Uninitialized;
                            }
                        }
                    }
                }
                Status::Committed(_) => (),
            }
        }
    }

    pub fn proposed_height(&self) -> Option<&u32> {
        match &self.status {
            Status::Processing(range) => Some(range.end()),
            _ => None,
        }
    }
}

fn creates_new_processing(existing: &u32, commit: &u32) -> Option<RangeInclusive<u32>> {
    let next = commit.checked_add(1)?;
    let prev_existing = existing.checked_sub(1)?;
    let r = next..=prev_existing;
    (!r.is_empty()).then_some(r)
}
