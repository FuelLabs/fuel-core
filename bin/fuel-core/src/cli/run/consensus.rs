#![allow(non_snake_case)]
//! Clap configuration related to consensus parameters

use anyhow::anyhow;
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
    pub fn leader_lock(&self) -> anyhow::Result<Option<RedisLeaderLockConfig>> {
        let LeaderLock {
            enabled,
            redis_urls,
            lease_key,
            lease_ttl,
            node_timeout,
            retry_delay,
            max_retry_delay_offset,
            max_attempts,
        } = self.leader_lock.clone();
        if enabled {
            Ok(Some(RedisLeaderLockConfig {
                redis_urls: redis_urls
                    .ok_or(anyhow!("`redis_urls` is required when `enabled` is true"))?,
                lease_key: lease_key
                    .ok_or(anyhow!("`lease_key` is required when `enabled` is true"))?,
                lease_ttl: lease_ttl
                    .ok_or(anyhow!("`lease_ttl` is required when `enabled` is true"))?
                    .into(),
                node_timeout: node_timeout.into(),
                retry_delay: retry_delay.into(),
                max_retry_delay_offset: max_retry_delay_offset.into(),
                max_attempts,
            }))
        } else {
            Ok(None)
        }
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
    /// Enable distributed leader lock for PoA producers.
    #[clap(
        long = "poa-leader-lock",
        env,
        default_value_t = false,
        requires_all = ["redis_urls", "lease_key", "lease_ttl"]
    )]
    enabled: bool,
    /// Redis URLs used for distributed leader locking across producers.
    /// Multiple values can be provided by repeating the arg.
    #[clap(long = "poa-leader-lock-redis-url", env, num_args = 1..)]
    redis_urls: Option<Vec<String>>,
    /// Redis key used to store the active leader lease.
    #[clap(long = "poa-leader-lock-key", env)]
    lease_key: Option<String>,
    /// Redis lease TTL for the leader lock.
    #[clap(long = "poa-leader-lock-ttl", env)]
    lease_ttl: Option<Duration>,
    /// Timeout per Redis node operation.
    #[clap(long = "poa-leader-lock-node-timeout", env, default_value = "100ms")]
    node_timeout: Duration,
    /// Base delay between Redlock retries.
    #[clap(long = "poa-leader-lock-retry-delay", env, default_value = "200ms")]
    retry_delay: Duration,
    /// Maximum random offset added to each retry delay.
    #[clap(
        long = "poa-leader-lock-max-retry-delay-offset",
        env,
        default_value = "100ms"
    )]
    max_retry_delay_offset: Duration,
    /// Maximum number of acquire attempts per can_produce_block call.
    #[clap(long = "poa-leader-lock-max-attempts", env, default_value_t = 3)]
    max_attempts: u32,
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
        assert!(command.trigger.clone().leader_lock().unwrap().is_none());
    }

    #[test]
    fn leader_lock__when_all_args_set_then_some() {
        let command = Command::try_parse_from([
            "",
            "--poa-leader-lock",
            "--poa-leader-lock-redis-url",
            "redis://127.0.0.1:6379/",
            "redis://127.0.0.1:6380/",
            "--poa-leader-lock-key",
            "poa:leader:lock",
            "--poa-leader-lock-ttl",
            "2s",
        ])
        .unwrap();
        let leader_lock = command.trigger.clone().leader_lock().unwrap().unwrap();
        assert_eq!(
            leader_lock.redis_urls,
            vec![
                "redis://127.0.0.1:6379/".to_string(),
                "redis://127.0.0.1:6380/".to_string()
            ]
        );
        assert_eq!(leader_lock.lease_key, "poa:leader:lock");
        assert_eq!(leader_lock.lease_ttl, StdDuration::from_secs(2));
        assert_eq!(leader_lock.node_timeout, StdDuration::from_millis(100));
        assert_eq!(leader_lock.retry_delay, StdDuration::from_millis(200));
        assert_eq!(
            leader_lock.max_retry_delay_offset,
            StdDuration::from_millis(100)
        );
        assert_eq!(leader_lock.max_attempts, 3);
    }

    #[test]
    fn leader_lock__when_enabled_without_required_fields_then_parse_error() {
        let result = Command::try_parse_from(["", "--poa-leader-lock"]);
        assert!(result.is_err());
    }
}
