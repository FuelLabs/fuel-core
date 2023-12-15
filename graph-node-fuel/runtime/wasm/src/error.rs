use wasmtime::Trap;

use graph::runtime::DeterministicHostError;

use crate::module::IntoTrap;

pub enum DeterminismLevel {
    /// This error is known to be deterministic. For example, divide by zero.
    /// TODO: For these errors, a further designation should be created about the contents
    /// of the actual message.
    Deterministic,

    /// This error is known to be non-deterministic. For example, an intermittent http failure.
    #[allow(dead_code)]
    NonDeterministic,

    /// The runtime is processing a given block, but there is an indication that the blockchain client
    /// might not consider that block to be on the main chain. So the block must be reprocessed.
    PossibleReorg,

    /// An error has not yet been designated as deterministic or not. This should be phased out over time,
    /// and is the default for errors like anyhow which are of an unknown origin.
    Unimplemented,
}

impl IntoTrap for DeterministicHostError {
    fn determinism_level(&self) -> DeterminismLevel {
        DeterminismLevel::Deterministic
    }
    fn into_trap(self) -> Trap {
        Trap::from(self.inner())
    }
}
