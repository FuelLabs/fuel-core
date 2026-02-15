#![allow(non_snake_case)]
//! Clap configuration related to consensus parameters

use clap::{
    ArgGroup,
    ValueEnum,
};
use fuel_core::service::config::{
    RedisLeaderLockConfig,
    Trigger as PoATrigger,
};
use humantime::Duration;

#[derive(Debug, Clone, clap::Args)]
pub struct PoATriggerArgs {
    #[clap(flatten)]
    instant: Instant,
    #[clap(flatten)]
    interval: Interval,
    #[clap(flatten)]
    open: Open,
    #[clap(flatten)]
    leader_lock: LeaderLock,
}

impl PoATriggerArgs {
    pub fn leader_lock(self) -> Option<RedisLeaderLockConfig> {
        self.leader_lock.into()
    }
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

#[derive(Debug, Clone, clap::Args)]
struct LeaderLock {
    /// Redis URL used for distributed leader locking across producers.
    #[clap(long = "poa-leader-lock-redis-url", env)]
    redis_url: Option<String>,
    /// Redis key used to store the active leader lease.
    #[clap(long = "poa-leader-lock-key", env)]
    lease_key: Option<String>,
    /// Redis lease TTL for the leader lock.
    #[clap(long = "poa-leader-lock-ttl", env)]
    lease_ttl: Option<Duration>,
}

impl From<LeaderLock> for Option<RedisLeaderLockConfig> {
    fn from(value: LeaderLock) -> Self {
        let LeaderLock {
            redis_url,
            lease_key,
            lease_ttl,
        } = value;
        match (redis_url, lease_key, lease_ttl) {
            (Some(redis_url), Some(lease_key), Some(lease_ttl)) => {
                Some(RedisLeaderLockConfig {
                    redis_url,
                    lease_key,
                    lease_ttl: lease_ttl.into(),
                })
            }
            _ => None,
        }
    }
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

    #[test]
    fn leader_lock__defaults_to_none() {
        let command = Command::try_parse_from([""]).unwrap();
        assert!(command.trigger.clone().leader_lock().is_none());
    }

    #[test]
    fn leader_lock__when_all_args_set_then_some() {
        let command = Command::try_parse_from([
            "",
            "--poa-leader-lock-redis-url",
            "redis://127.0.0.1:6379/",
            "--poa-leader-lock-key",
            "poa:leader:lock",
            "--poa-leader-lock-ttl",
            "2s",
        ])
        .unwrap();
        let leader_lock = command.trigger.clone().leader_lock().unwrap();
        assert_eq!(leader_lock.redis_url, "redis://127.0.0.1:6379/");
        assert_eq!(leader_lock.lease_key, "poa:leader:lock");
        assert_eq!(leader_lock.lease_ttl, StdDuration::from_secs(2));
    }
}
