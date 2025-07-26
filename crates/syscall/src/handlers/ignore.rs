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
#[derive(Debug, Clone, Default)]
pub struct IgnoreEcal {
    /// If this is true, the ECAL invocations will be ignored.
    /// If false, the ECAL invocations will error.
    pub enabled: bool,
}

impl EcalHandler for IgnoreEcal {
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
        if !vm.ecal_state().enabled {
            return Err(PanicReason::EcalError.into());
        }
        Ok(())
    }
}
