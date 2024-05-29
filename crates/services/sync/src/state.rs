//! State of the sync service.

use std::{
    cmp::Ordering,
    ops::RangeInclusive,
};

#[cfg(test)]
mod test;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// State of the sync service.
///
/// The state takes evidence and produces a status.
pub struct State {
    status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Status of the sync service.
pub enum Status {
    /// The service is not initialized and there is nothing to sync.
    Uninitialized,
    /// This range is being processed.
    Processing(RangeInclusive<u32>),
    /// This height is committed.
    Committed(u32),
}

impl State {
    #[tracing::instrument(skip_all)]
    /// Create a new state from the current committed and observed heights.
    pub fn new(
        committed: impl Into<Option<u32>>,
        observed: impl Into<Option<u32>>,
    ) -> Self {
        let status = match (committed.into(), observed.into()) {
            // Both the committed and observed heights are known.
            (Some(committed), Some(observed)) => {
                // If there is a gap between the committed and observed heights,
                // the service is processing the gap otherwise the service is
                // has nothing to sync.
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
            // Only the committed height is known, so the service has nothing to sync.
            (Some(committed), None) => Status::Committed(committed),
            // Only the observed height is known, so the service is processing
            // up to that height.
            (None, Some(observed)) => Status::Processing(0..=observed),
            // No heights are known, so the service is uninitialized.
            (None, None) => Status::Uninitialized,
        };
        tracing::debug!("Initial status: {:?}", status);
        Self { status }
    }

    #[tracing::instrument]
    /// Get the current range to process.
    pub fn process_range(&self) -> Option<RangeInclusive<u32>> {
        match &self.status {
            Status::Processing(range) => {
                tracing::debug!("Processing range: {:?}", range);
                Some(range.clone())
            }
            _ => {
                tracing::debug!("Nothing to process");
                None
            }
        }
    }

    #[tracing::instrument]
    /// Record that a block has been committed.
    pub fn commit(&mut self, height: u32) {
        let new_status = match &self.status {
            // Currently processing a range and recording a commit.
            Status::Processing(range) => {
                if height < *range.start() {
                    // The committed height is less than the start of the processing range,
                    // it is a lag between committing and processing.
                    None
                } else {
                    match height.cmp(range.end()) {
                        // The commit is less than the end of the range, so the range
                        // is still being processed.
                        Ordering::Less => Some(Status::Processing(
                            height.saturating_add(1)..=*range.end(),
                        )),
                        // The commit is equal or greater than the end of the range,
                        // so the range is fully committed.
                        Ordering::Equal | Ordering::Greater => {
                            Some(Status::Committed(height))
                        }
                    }
                }
            }
            // Currently uninitialized so now are committed.
            Status::Uninitialized => Some(Status::Committed(height)),
            // Currently committed and recording a commit.
            Status::Committed(existing) => {
                // Take the max of the existing and new commits.
                match height.cmp(existing) {
                    Ordering::Less | Ordering::Equal => None,
                    Ordering::Greater => Some(Status::Committed(height)),
                }
            }
        };
        self.apply_status(new_status);
    }

    #[tracing::instrument]
    /// Record that a block has been observed.
    pub fn observe(&mut self, height: u32) -> bool {
        let new_status = match &self.status {
            // Currently uninitialized so process from the start to the observed height.
            Status::Uninitialized => Some(Status::Processing(0..=height)),
            // Currently processing a range and recording an observation.
            Status::Processing(range) => match range.end().cmp(&height) {
                // The range end is less than the observed height, so
                // extend the range to the observed height.
                Ordering::Less => Some(Status::Processing(*range.start()..=height)),
                // The range end is equal or greater than the observed height,
                // so ignore it.
                Ordering::Equal | Ordering::Greater => None,
            },
            // Currently committed and recording an observation.
            // If there is a gap between the committed and observed heights,
            // the service is processing.
            Status::Committed(committed) => committed.checked_add(1).and_then(|next| {
                let r = next..=height;
                (!r.is_empty()).then_some(Status::Processing(r))
            }),
        };
        let status_change = new_status.is_some();
        self.apply_status(new_status);
        status_change
    }

    #[tracing::instrument]
    /// Record that a range of blocks have failed to process.
    pub fn failed_to_process(&mut self, range: RangeInclusive<u32>) {
        // Ignore empty ranges.
        let status = (!range.is_empty())
            .then_some(())
            .and_then(|_| match &self.status {
                // Currently uninitialized or committed.
                // Failures do not override these status.
                Status::Uninitialized | Status::Committed(_) => None,
                // Currently processing a range and recording a failure.
                Status::Processing(processing) => range
                    // If the failure range contains the start of the processing range,
                    // then there is no reason to continue trying to process this range.
                    // The processing range is reverted back to just before it's start.
                    // The revert is either to the last committed height, or to uninitialized.
                    .contains(processing.start())
                    .then(|| {
                        processing
                            .start()
                            .checked_sub(1)
                            .map_or(Status::Uninitialized, Status::Committed)
                    })
                    .or_else(|| {
                        // If the failure range contains the end of the processing range,
                        // or the processing range contains the start of the failure range,
                        // then the processing range is shortened to just before the failure range.
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
                        // If the processing range contains the end of the failure range,
                        // then the entire processing range is failed and reverted back to
                        // the last committed height, or to uninitialized.
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

    fn apply_status(&mut self, status: Option<Status>) {
        match status {
            Some(s) => {
                tracing::info!("Status change from: {:?}, to: {:?}", self.status, s);
                self.status = s;
            }
            _ => {
                tracing::debug!("No status change: {:?}", self.status);
            }
        }
    }

    #[cfg(test)]
    /// Get the current observed height.
    pub fn proposed_height(&self) -> Option<&u32> {
        match &self.status {
            Status::Processing(range) => Some(range.end()),
            _ => None,
        }
    }
}
