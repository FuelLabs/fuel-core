mod combinators;
mod costs;
mod ops;
mod saturating;
mod size_of;
use crate::components::metrics::gas::GasMetrics;
use crate::prelude::{CheapClone, ENV_VARS};
use crate::runtime::DeterministicHostError;
pub use combinators::*;
pub use costs::DEFAULT_BASE_COST;
pub use costs::*;
pub use saturating::*;

use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::Arc;
use std::{fmt, fmt::Display};

pub struct GasOp {
    base_cost: u64,
    size_mult: u64,
}

impl GasOp {
    pub fn with_args<T, C>(&self, c: C, args: T) -> Gas
    where
        Combine<T, C>: GasSizeOf,
    {
        Gas(self.base_cost) + Combine(args, c).gas_size_of() * self.size_mult
    }
}

/// Sort of a base unit for gas operations. For example, if one is operating
/// on a BigDecimal one might like to know how large that BigDecimal is compared
/// to other BigDecimals so that one could to (MultCost * gas_size_of(big_decimal))
/// and re-use that logic for (WriteToDBCost or ReadFromDBCost) rather than having
/// one-offs for each use-case.
/// This is conceptually much like CacheWeight, but has some key differences.
/// First, this needs to be stable - like StableHash (same independent of
/// platform/compiler/run). Also this can be somewhat context dependent. An example
/// of context dependent costs might be if a value is being hex encoded or binary encoded
/// when serializing.
///
/// Either implement gas_size_of or const_gas_size_of but never none or both.
pub trait GasSizeOf {
    #[inline(always)]
    fn gas_size_of(&self) -> Gas {
        Self::const_gas_size_of().expect("GasSizeOf unimplemented")
    }
    /// Some when every member of the type has the same gas size.
    #[inline(always)]
    fn const_gas_size_of() -> Option<Gas> {
        None
    }
}

/// This wrapper ensures saturating arithmetic is used
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Gas(u64);

impl Gas {
    pub const ZERO: Gas = Gas(0);

    pub const fn new(gas: u64) -> Self {
        Gas(gas)
    }

    #[cfg(debug_assertions)]
    pub const fn value(&self) -> u64 {
        self.0
    }
}

impl Display for Gas {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.0.fmt(f)
    }
}

#[derive(Clone)]
pub struct GasCounter {
    counter: Arc<AtomicU64>,
    metrics: GasMetrics,
}

impl CheapClone for GasCounter {}

impl GasCounter {
    pub fn new(metrics: GasMetrics) -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(0)),
            metrics,
        }
    }

    /// This should be called once per host export
    pub fn consume_host_fn_inner(
        &self,
        mut amount: Gas,
        method: Option<&str>,
    ) -> Result<(), DeterministicHostError> {
        amount += costs::HOST_EXPORT_GAS;

        // If gas metrics are enabled, track the gas used
        if ENV_VARS.enable_gas_metrics {
            if let Some(method) = method {
                self.metrics.track_gas(method, amount.0);
                self.metrics.track_operations(method, 1);
            }
        }

        let old = self
            .counter
            .fetch_update(SeqCst, SeqCst, |v| Some(v.saturating_add(amount.0)))
            .unwrap();
        let new = old.saturating_add(amount.0);
        if new >= ENV_VARS.max_gas_per_handler {
            Err(DeterministicHostError::gas(anyhow::anyhow!(
                "Gas limit exceeded. Used: {}",
                new
            )))
        } else {
            Ok(())
        }
    }

    pub fn consume_host_fn(&self, amount: Gas) -> Result<(), DeterministicHostError> {
        self.consume_host_fn_inner(amount, Some("untracked"))
    }

    pub fn consume_host_fn_with_metrics(
        &self,
        amount: Gas,
        method: &str,
    ) -> Result<(), DeterministicHostError> {
        self.consume_host_fn_inner(amount, Some(method))
    }

    pub fn get(&self) -> Gas {
        Gas(self.counter.load(SeqCst))
    }
}
