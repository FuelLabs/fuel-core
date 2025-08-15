//! Syscall handler that ignores all ECAL invocations when enabled.

use fuel_core_types::{
    fuel_asm::RegId,
    fuel_tx::PanicReason,
    fuel_vm::{
        Interpreter,
        error::SimpleResult,
        interpreter::{
            EcalHandler,
            Memory,
        },
    },
};

/// If enabled, ignores all ECAL invocations.
#[derive(Debug, Clone)]
pub enum PossiblyIgnoreEcal {
    /// Ignore ECAL invocations.
    Ignore,
    /// Error on ECAL invocations, just like the default behavior.
    Error,
}

impl EcalHandler for PossiblyIgnoreEcal {
    fn ecal<M, S, Tx, V>(
        vm: &mut Interpreter<M, S, Tx, Self, V>,
        _a: RegId,
        _b: RegId,
        _c: RegId,
        _d: RegId,
    ) -> SimpleResult<()>
    where
        M: Memory,
    {
        match vm.ecal_state() {
            Self::Ignore => Ok(()),
            Self::Error => Err(PanicReason::EcalError.into()),
        }
    }
}
