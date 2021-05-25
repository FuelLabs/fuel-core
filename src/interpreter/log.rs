use super::Interpreter;
use crate::consts::*;

use fuel_asm::{RegisterId, Word};
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LogEvent {
    Register {
        pc: Word,
        register: RegisterId,
        value: Word,
    },
}

impl Interpreter {
    pub fn log(&self) -> &[LogEvent] {
        self.log.as_slice()
    }

    pub fn log_append(&mut self, reg: &[RegisterId]) -> bool {
        let pc = self.registers[REG_PC];
        let registers = &self.registers;
        let log = &mut self.log;

        let entries = reg.iter().filter(|r| r > &&0).filter_map(|r| {
            registers.get(*r).map(|v| {
                let log = LogEvent::Register {
                    pc,
                    register: *r,
                    value: *v,
                };

                debug!("Appending log {:?}", log);
                log
            })
        });

        log.extend(entries);

        true
    }
}
