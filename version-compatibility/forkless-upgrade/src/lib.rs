#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[cfg(test)]
mod backward_compatibility;
#[cfg(test)]
mod forward_compatibility;
#[cfg(test)]
pub(crate) mod tests_helper;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
