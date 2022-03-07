use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpSocket, TcpStream};


use fuel_types::Word;
use fuel_vm::prelude::{Breakpoint, Interpreter, Receipt, ContractId};

pub enum CommandControlFlow {
    /// Debugger awaiting for more commands
    Debugger,
    /// Run until breakpoint or termination
    Proceed,
    /// Terminate immediately
    Terminate,
}

/// Debugger commands
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Command {
    /// Requests version info of the node, so that
    /// the debugger can ensure it's interoperable
    Version,
    /// Runs the program until any pause condition
    /// 1. an error occurs
    /// 2. a breapoint is encountered
    /// 3. the program ends
    Continue,
    /// Turn single-stepping mode on or off
    SingleStepping(bool),
    /// Sets a breakpoint on given address
    Breakpoint(Breakpoint),
    /// Requests a register dump
    ReadRegisters,
    /// Requests a memory dump
    ReadMemory {
        start: usize,
        len: usize,
    },
}
impl Command {
    pub fn execute<S>(&self, vm: &mut Interpreter<S>) -> (Option<Response>, CommandControlFlow) {
        match self {
            Self::Version => (
                Some(Response::Version {
                    core: env!("CARGO_PKG_VERSION").to_owned(),
                }),
                CommandControlFlow::Debugger,
            ),
            Self::Continue => (None, CommandControlFlow::Proceed),
            Self::SingleStepping(value) => {
                todo!("Single-stepping on the VM");
                // vm.set_single_stepping(value);
                (None, CommandControlFlow::Debugger)
            },
            Self::Breakpoint(b) => {
                vm.set_breakpoint((*b).into());
                (Some(Response::Ok), CommandControlFlow::Debugger)
            }
            Self::ReadRegisters => {
                let regs: Vec<_> = vm.registers().iter().copied().collect();
                (
                    Some(Response::ReadRegisters(regs)),
                    CommandControlFlow::Debugger,
                )
            }
            Self::ReadMemory  {start, len} => {
                let regs: Vec<_> = vm.memory().iter().skip(*start).take(*len).copied().collect();
                (
                    Some(Response::ReadMemory(regs)),
                    CommandControlFlow::Debugger,
                )
            }
        }
    }
}

/// Reponses to commands
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Response {
    /// Command executed successfully
    Ok,
    /// Program terminated
    Terminated {
        receipts: Vec<Receipt>,
    },
    /// Version reply
    Version { core: String },
    /// A breakpoint was encountered
    Breakpoint(Breakpoint),
    /// A register dump
    ReadRegisters(Vec<Word>),
    /// A memory dump
    ReadMemory(Vec<u8>),
}

pub async fn process<S>(ds: &mut TcpStream, vm: &mut Interpreter<S>, event: Option<Response>) {
    let (reader, mut writer) = ds.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    if let Some(r) = event {
        let mut v = serde_json::to_string(&r).expect("Serialization failed");
        v.push('\n');
        writer.write(v.as_bytes()).await.expect("Sending failed");
    }

    while let Ok(_) = reader.read_line(&mut line).await {
        let cmd: Command =
            serde_json::from_str(&line).expect("Invalid JSON from the debugger");
        line.clear();

        let (resp, cf) = cmd.execute(vm);

        if let Some(r) = resp {
            let mut v = serde_json::to_string(&r).expect("Serialization failed");
            v.push('\n');
            writer.write(v.as_bytes()).await.expect("Sending failed");
        }

        match cf {
            CommandControlFlow::Debugger => continue,
            CommandControlFlow::Proceed => break,
            CommandControlFlow::Terminate => todo!("debugger termination"),
        }
    }
}
