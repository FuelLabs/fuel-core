//! Clap configuration related to consensus parameters

use clap::{
    ArgGroup,
    ValueEnum,
};
use fuel_core::service::config::Trigger as PoATrigger;
use humantime::Duration;

#[derive(Debug, Clone, clap::Args)]
pub struct PoATriggerArgs {
    #[clap(flatten)]
    instant: Instant,
    #[clap(flatten)]
    hybrid: Hybrid,
    #[clap(flatten)]
    interval: Interval,
}

// Convert from arg struct to PoATrigger enum
impl From<PoATriggerArgs> for PoATrigger {
    fn from(value: PoATriggerArgs) -> Self {
        match value {
            PoATriggerArgs {
                hybrid:
                    Hybrid {
                        idle_time: Some(idle_time),
                        min_time: Some(min_time),
                        max_time: Some(max_time),
                    },
                ..
            } => PoATrigger::Hybrid {
                min_block_time: min_time.into(),
                max_tx_idle_time: idle_time.into(),
                max_block_time: max_time.into(),
            },
            PoATriggerArgs {
                interval: Interval { period: Some(p) },
                ..
            } => PoATrigger::Interval {
                block_time: p.into(),
            },
            PoATriggerArgs { instant, .. } if instant.instant == Boolean::True => {
                PoATrigger::Instant
            }
            _ => PoATrigger::Never,
        }
    }
}

#[derive(Debug, Clone, clap::Args)]
#[clap(
    group = ArgGroup::new("instant-mode").args(&["instant"]).conflicts_with_all(&["interval-mode", "hybrid-mode"]),
)]
struct Instant {
    /// Use instant block production mode.
    /// Newly submitted txs will immediately trigger the production of the next block.
    /// Cannot be combined with other poa flags.
    #[arg(long = "poa-instant", default_value = "true", value_parser)]
    instant: Boolean,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Boolean {
    True,
    False,
}

#[derive(Debug, Clone, clap::Args)]
#[clap(
    group = ArgGroup::new("hybrid-mode")
            .args(&["min_time", "idle_time", "max_time"])
            .multiple(true)
            .conflicts_with_all(&["interval-mode", "instant-mode"]),
)]
struct Hybrid {
    /// Hybrid trigger option.
    /// Sets a minimum lower bound between blocks. This should be set high enough to ensure
    /// peers can sync the blockchain.
    /// Cannot be combined with other poa mode options (instant or interval).
    #[arg(long = "poa-hybrid-min-time", requires_all = ["idle_time", "max_time"])]
    min_time: Option<Duration>,
    /// Hybrid trigger option.
    /// Sets the max time block production will wait after a period of inactivity before producing
    /// a new block. If there are txs available but not enough for a full block,
    /// this is how long the trigger will wait for more txs.
    /// This ensures that if a burst of transactions are submitted,
    /// they will all be included into the next block instead of making a new block immediately and
    /// then waiting for the minimum block time to process the rest.
    /// Cannot be combined with other poa mode options (instant or interval).
    #[arg(long = "poa-hybrid-idle-time", requires_all = ["min_time", "max_time"])]
    idle_time: Option<Duration>,
    /// Hybrid trigger option.
    /// Sets the maximum time block production will wait to produce a block (even if empty). This
    /// ensures that there is a regular cadence even under sustained load.
    /// Cannot be combined with other poa mode options (instant or interval).
    #[arg(long = "poa-hybrid-max-time", requires_all = ["min_time", "idle_time"])]
    max_time: Option<Duration>,
}

#[derive(Debug, Clone, clap::Args)]
#[clap(
    group = ArgGroup::new("interval-mode").args(&["period"]).conflicts_with_all(&["instant-mode", "hybrid-mode"]),
)]
struct Interval {
    /// Interval trigger option.
    /// Produces blocks on a fixed interval regardless of txpool activity.
    /// Cannot be combined with other poa flags.
    #[clap(long = "poa-interval-period")]
    pub period: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use fuel_core::service::config::Trigger;
    use std::time::Duration as StdDuration;
    use test_case::test_case;

    #[derive(Debug, Clone, Parser)]
    pub struct Command {
        #[clap(flatten)]
        trigger: PoATriggerArgs,
    }

    pub fn hybrid(min_t: u64, mi_t: u64, max_t: u64) -> Trigger {
        Trigger::Hybrid {
            min_block_time: StdDuration::from_secs(min_t),
            max_tx_idle_time: StdDuration::from_secs(mi_t),
            max_block_time: StdDuration::from_secs(max_t),
        }
    }

    #[test_case(&[] => Ok(Trigger::Instant); "defaults to instant trigger")]
    #[test_case(&["", "--poa-instant=false"] => Ok(Trigger::Never); "never trigger if instant is explicitly disabled")]
    #[test_case(&["", "--poa-interval-period=1s"] => Ok(Trigger::Interval { block_time: StdDuration::from_secs(1)}); "uses interval mode if set")]
    #[test_case(&["", "--poa-hybrid-min-time=1s", "--poa-hybrid-idle-time=2s", "--poa-hybrid-max-time=3s"] => Ok(hybrid(1,2,3)); "uses hybrid mode if set")]
    #[test_case(&["", "--poa-interval-period=1s", "--poa-hybrid-min-time=1s", "--poa-hybrid-idle-time=2s", "--poa-hybrid-max-time=3s"] => Err(()); "can't set hybrid and interval at the same time")]
    #[test_case(&["", "--poa-instant=true", "--poa-hybrid-min-time=1s", "--poa-hybrid-idle-time=2s", "--poa-hybrid-max-time=3s"] => Err(()); "can't set hybrid and instant at the same time")]
    #[test_case(&["", "--poa-instant=true", "--poa-interval-period=1s"] => Err(()); "can't set interval and instant at the same time")]
    #[test_case(&["", "--poa-hybrid-min-time=1s"] => Err(()); "can't set hybrid min time without idle and max")]
    fn parse(args: &[&str]) -> Result<Trigger, ()> {
        Command::try_parse_from(args)
            .map_err(|_| ())
            .map(|c| c.trigger.into())
    }
}
