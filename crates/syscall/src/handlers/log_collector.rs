//! Syscall handler that allows log emit endpoint using ECAL invocation when enabled.

use alloc::{
    borrow::ToOwned,
    collections::BTreeMap,
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

/// Source of the log: either a predicate (with its index) or a script.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogSource {
    /// The source of the log is a predicate with the given index.
    Predicate(usize),
    /// The source of the log is the script.
    Script,
}

/// Collects logs from ECAL invocations.
#[derive(Debug, Clone, Default)]
pub struct EcalLogCollector {
    /// If this is true, the ECAL invocations will be logged.
    /// If false, the ECAL invocations error.
    pub enabled: bool,
    /// Logs collected from ECAL invocations.
    pub logs: Arc<Mutex<BTreeMap<LogSource, Vec<LogEntry>>>>,
}

/// Log entry from ECAL.
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

                let source = match predicate_idx {
                    Some(idx) => LogSource::Predicate(idx),
                    None => LogSource::Script,
                };

                vm.ecal_state_mut()
                    .logs
                    .lock()
                    .entry(source)
                    .or_default()
                    .push(LogEntry { pc, fd, message });
            }
            _ => {
                return Err(PanicReason::EcalError.into());
            }
        };

        Ok(())
    }
}

impl EcalLogCollector {
    /// Print ECAL/syscall logs if any produced.
    pub fn maybe_print_logs(self, span: tracing::Span) {
        for (source, logs) in self.logs.lock().iter() {
            let logs_string = logs
                .iter()
                .map(|log| {
                    format!(
                        "[{}, pc: {:>10x}] {}: {}",
                        match source {
                            LogSource::Predicate(idx) => format!("predicate {idx}"),
                            LogSource::Script => "script".to_owned(),
                        },
                        log.pc,
                        match log.fd {
                            1 => "stdout".to_owned(),
                            2 => "stderr".to_owned(),
                            other => format!("fd {}", other),
                        },
                        log.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            span.in_scope(|| {
                tracing::info!("\n{logs_string}");
            });
        }
    }
}
