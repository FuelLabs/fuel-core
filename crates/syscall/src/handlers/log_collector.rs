//! Syscall handler that allows log emit endpoint using ECAL invocation when enabled.

use alloc::{
    borrow::ToOwned,
    format,
    string::String,
    sync::Arc,
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
use parking_lot::Mutex;

/// Collects logs from ECAL invocations.
#[derive(Debug, Clone, Default)]
pub struct EcalLogCollector {
    /// If this is true, the ECAL invocations will be logged.
    /// If false, the ECAL invocations error.
    pub enabled: bool,
    /// Logs collected from ECAL invocations.
    pub logs: Arc<Mutex<Vec<LogEntry>>>,
}

/// Log entry from ECAL.
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// If the log was preoduced by a predicate, this is the index of that predicate.
    pub predicate_idx: Option<usize>,
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
                let predicate_idx = vm.context().predicate().map(|p| p.idx());
                let pc = regs[RegId::PC];
                let fd = regs[b];
                let addr = regs[c];
                let size = regs[d];
                let bytes = vm.memory().read(addr, size)?;
                let message = core::str::from_utf8(bytes)
                    .map_err(|_| PanicReason::EcalError)?
                    .to_owned();

                vm.ecal_state_mut().logs.lock().push(LogEntry {
                    predicate_idx,
                    pc,
                    fd,
                    message,
                });
            }
            _ => {
                return Err(PanicReason::EcalError.into());
            }
        };

        Ok(())
    }
}

/// Print ECAL/syscall logs if any produced.
pub fn maybe_print_logs(logs: &[LogEntry], span: tracing::Span) {
    if !logs.is_empty() {
        let logs_string = logs
            .iter()
            .map(|entry| {
                format!(
                    "[{}, pc: {:>10x}] {}: {}",
                    match entry.predicate_idx {
                        Some(idx) => format!("predicate {idx}"),
                        None => "script".to_owned(),
                    },
                    entry.pc,
                    match entry.fd {
                        1 => "stdout".to_owned(),
                        2 => "stderr".to_owned(),
                        other => format!("fd {}", other),
                    },
                    entry.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        span.in_scope(|| {
            tracing::info!("\n{logs_string}");
        });
    }
}
