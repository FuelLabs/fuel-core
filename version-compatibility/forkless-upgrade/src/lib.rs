#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[cfg(test)]
use netlink_proto as _;

#[cfg(test)]
mod backward_compatibility;
#[cfg(test)]
mod forward_compatibility;

#[cfg(test)]
mod gas_price_algo_compatibility;

#[cfg(test)]
mod genesis;
#[cfg(test)]
pub(crate) mod tests_helper;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
