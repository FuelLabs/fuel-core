use alloc::{
    borrow::ToOwned,
    string::String,
    vec::Vec,
};
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

#[derive(Debug, Clone, Default)]
pub struct EcalLogCollector {
    pub enabled: bool,
    logs: Vec<LogEntry>,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Program counter at the time of the log
    pub pc: u64,
    /// Stream identifier (e.g., file descriptor)
    pub fd: u64,
    /// Log message
    pub message: String,
}

/// Syscall ID for logging operation.
pub const LOG_SYSCALL: u64 = 1000;
/// File descriptor for standard output.
pub const STDOUT: u64 = 1;
/// File descriptor for standard error.
pub const STDERR: u64 = 2;

impl EcalLogCollector {
    pub fn logs(&self) -> &[LogEntry] {
        &self.logs
    }
}

impl EcalHandler for EcalLogCollector {
    fn ecal<M, S, Tx, V>(
        vm: &mut Interpreter<M, S, Tx, Self, V>,
        a: RegId,
        b: RegId,
        c: RegId,
        d: RegId,
    ) -> SimpleResult<()>
    where
        M: Memory,
    {
        if !vm.ecal_state().enabled {
            return Err(PanicReason::EcalError.into());
        }

        let regs = vm.registers();
        match regs[a] {
            LOG_SYSCALL => {
                let pc = regs[RegId::PC];
                let fd = regs[b];
                let addr = regs[c];
                let size = regs[d];
                let bytes = vm.memory().read(addr, size)?;
                let message = core::str::from_utf8(bytes)
                    .map_err(|_| PanicReason::EcalError)?
                    .to_owned();

                vm.ecal_state_mut().logs.push(LogEntry { pc, fd, message });
            }
            _ => {
                return Err(PanicReason::EcalError.into());
            }
        };

        Ok(())
    }
}
