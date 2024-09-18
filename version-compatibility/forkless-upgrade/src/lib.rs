#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[cfg(test)]
mod backward_compatibility;
// TODO: Uncomment forward compatibility tests after release of the `fuel-core 0.36.0`.
//  New forward compatibility test should use the `fuel-core 0.36.0`.
//  https://github.com/FuelLabs/fuel-core/issues/2198
// #[cfg(test)]
// mod forward_compatibility;
#[cfg(test)]
pub(crate) mod tests_helper;

#[cfg(test)]
fuel_core_trace::enable_tracing!();
