#![deny(unused_crate_dependencies)]
#![deny(warnings)]

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

#[cfg(test)]
/// Old versions of the `fuel-core` doesn't expose the port used by the P2P.
/// So, if we use `--peering-port 0`, we don't know what port is assign to which node.
/// Some tests require to know the port to use the node as a bootstrap(or reserved node).
/// This function generates a port based on the seed, where seed based on the file, line,
/// and column. While it is possible that we will generate the same port for several tests,
/// it is done at least in a deterministic way, so we should be able to reproduce
/// the error locally.
fn select_port(seed: String) -> &'static str {
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };
    use std::hash::{
        DefaultHasher,
        Hash,
        Hasher,
    };

    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let hash = hasher.finish();

    let mut rng = StdRng::seed_from_u64(hash);
    let port: u16 = rng.gen_range(500..65000);

    port.to_string().leak()
}
