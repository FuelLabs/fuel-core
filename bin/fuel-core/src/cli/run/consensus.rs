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
    interval: Interval,
    #[clap(flatten)]
    open: Open,
}

// Convert from arg struct to PoATrigger enum
impl From<PoATriggerArgs> for PoATrigger {
    fn from(value: PoATriggerArgs) -> Self {
        match value {
            PoATriggerArgs {
                open: Open { period: Some(p) },
                ..
            } => PoATrigger::Open { period: p.into() },
            PoATriggerArgs {
                interval:
                    Interval {
                        block_time: Some(p),
                    },
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
    group = ArgGroup::new("instant-mode").args(&["instant"]).conflicts_with_all(&["interval-mode", "open-mode"]),
)]
struct Instant {
    /// Use instant block production mode.
    /// Newly submitted txs will immediately trigger the production of the next block.
    /// Cannot be combined with other poa flags.
    #[arg(long = "poa-instant", default_value = "true", value_parser, env)]
    instant: Boolean,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Boolean {
    True,
    False,
}

#[derive(Debug, Clone, clap::Args)]
#[clap(
    group = ArgGroup::new("interval-mode").args(&["block_time"]).conflicts_with_all(&["instant-mode", "open-mode"]),
)]
struct Interval {
    /// Interval trigger option.
    /// Produces blocks on a fixed interval regardless of txpool activity.
    /// Cannot be combined with other poa flags.
    #[clap(long = "poa-interval-period", env)]
    pub block_time: Option<Duration>,
}

#[derive(Debug, Clone, clap::Args)]
#[clap(
    group = ArgGroup::new("open-mode").args(&["period"]).conflicts_with_all(&["instant-mode", "interval-mode"]),
)]
struct Open {
    /// Opens the block production immediately and keeps it open for the specified period.
    /// After period is over, the block is produced.
    /// Cannot be combined with other poa flags.
    #[clap(long = "poa-open-period", env)]
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

    #[test_case(&[] => Ok(Trigger::Instant); "defaults to instant trigger")]
    #[test_case(&["", "--poa-instant=false"] => Ok(Trigger::Never); "never trigger if instant is explicitly disabled")]
    #[test_case(&["", "--poa-interval-period=1s"] => Ok(Trigger::Interval { block_time: StdDuration::from_secs(1)}); "uses interval mode if set")]
    #[test_case(&["", "--poa-open-period=1s"] => Ok(Trigger::Open { period: StdDuration::from_secs(1)}); "uses open mode if set")]
    #[test_case(&["", "--poa-instant=true", "--poa-interval-period=1s"] => Err(()); "can't set interval and instant at the same time")]
    #[test_case(&["", "--poa-open-period=1s", "--poa-interval-period=1s"] => Err(()); "can't set open and interval at the same time")]
    fn parse(args: &[&str]) -> Result<Trigger, ()> {
        Command::try_parse_from(args)
            .map_err(|_| ())
            .map(|c| c.trigger.into())
    }
}
