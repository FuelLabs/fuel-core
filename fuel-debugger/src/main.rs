use clap::Parser;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::net::SocketAddr;
use std::str::FromStr;
use std::{env, io, net, path::PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{
    tcp::{ReadHalf, WriteHalf},
    TcpListener, TcpStream,
};

use fuel_core::debugger::{Command, Response};
use fuel_vm::consts::{VM_MAX_RAM, WORD_SIZE};
use fuel_vm::prelude::{Breakpoint, ContractId, Interpreter, Receipt};

use fuel_debugger::{names, Client, Listener};

#[derive(Parser, Debug)]
pub struct Opt {
    #[clap(long = "ip", default_value = "127.0.0.1", parse(try_from_str))]
    pub ip: net::IpAddr,

    #[clap(long = "port", default_value = "4001")]
    pub port: u16,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let config = Opt::parse();

    let mut rl = Editor::<()>::new();

    let listener = Listener::new(TcpListener::bind((config.ip, config.port)).await?);

    println!(
        "Listening for connections at {:?}",
        SocketAddr::from((config.ip, config.port))
    );

    loop {
        let (mut client, addr) = listener.accept().await?;
        println!("Connected (remote {:?})", addr);

        loop {
            let user_command = match rl.readline(">> ") {
                Ok(line) => {
                    rl.add_history_entry(line.as_str());
                    parse_and_run_command(&mut client, line.as_str()).await;
                }
                Err(err) => {
                    println!("{:?}", err);
                    break;
                }
            };
        }
        println!("Disconnected");
    }
}

macro_rules! ensure_no_more_args {
    ($it:expr) => {
        if $it.next().is_some() {
            println!("Too many arguments for this command");
            return Ok(());
        }
    };
}

async fn parse_and_run_command(client: &mut Client, text: &str) -> io::Result<()> {
    let mut split = text.trim().split_ascii_whitespace();

    if let Some(cmd) = split.next() {
        match cmd {
            "h" | "help" => {
                println!(
                    "{}",
                    [
                        "help                           - print this help message",
                        "quit                           - close the debugger, unpausing execution",
                        "version                        - query version information",
                        "continue                       - run until next breakpoint or end",
                        "step [on|off]                  - turn single-stepping on or off",
                        "breakpoint [contractid] offset - set a breakpoint",
                        "registers [regname ...]        - dump registers",
                        "memory [ [offset] limit ]      - dump memory",
                    ]
                    .join("\n")
                );
            }
            "q" | "quit" => {
                ensure_no_more_args!(split);
                todo!("quit gracefully");
            }
            "v" | "version" => {
                ensure_no_more_args!(split);
                println!("{:?}", client.cmd(&Command::Version).await?);
            }
            "c" | "continue" => {
                ensure_no_more_args!(split);
                println!("{:?}", client.cmd(&Command::Continue).await?);
            }
            "s" | "step" => {
                println!(
                    "{:?}",
                    client
                        .cmd(&Command::SingleStepping(
                            split
                                .next()
                                .map(|v| ["off", "no", "disable"].contains(&v))
                                .unwrap_or(false),
                        ))
                        .await?
                );
            }
            "b" | "breakpoint" => {
                let first = if let Some(first) = split.next() {
                    first
                } else {
                    println!("Required at least one argument");
                    return Ok(());
                };
                let b = if let Some(second) = split.next() {
                    if let Ok(contract_id) = first.parse::<ContractId>() {
                        if let Some(offset) = parse_int(second) {
                            Breakpoint::new(contract_id, offset as u64)
                        } else {
                            println!("Invalid argument");
                            return Ok(());
                        }
                    } else {
                        println!("Invalid argument");
                        return Ok(());
                    }
                } else {
                    if let Some(offset) = parse_int(first) {
                        Breakpoint::script(offset as u64)
                    } else {
                        println!("Invalid argument");
                        return Ok(());
                    }
                };

                println!("{:?}", client.cmd(&Command::Breakpoint(b)).await?);
            }
            "r" | "registers" => {
                let regs = match client.cmd(&Command::ReadRegisters).await? {
                    Response::ReadRegisters(regs) => regs,
                    other => panic!("Unexpected response {:?}", other),
                };

                let mut any_specified = false;
                while let Some(arg) = split.next() {
                    any_specified = true;

                    if let Some(v) = parse_int(arg) {
                        if v < regs.len() {
                            println!("{:?}", regs[v]);
                        } else {
                            println!("Register index too large {}", v);
                            return Ok(());
                        }
                    } else if let Some(i) = names::REGISTERS.get(&arg) {
                        println!("{:?}", regs[*i]);
                    } else {
                        println!("Unknown register name {}", arg);
                        return Ok(());
                    }
                }

                if !any_specified {
                    println!("{:?}", regs);
                }
            }
            "m" | "memory" => {
                let a = split.next();
                let b = split.next();

                let (offset, limit) = if let Some(limit) = b {
                    if let Some(off) = parse_int(a.unwrap()) {
                        if let Some(lim) = parse_int(limit) {
                            (off, Some(lim))
                        } else {
                            println!("Invalid argument: limit");
                            return Ok(());
                        }
                    } else {
                        println!("Invalid argument: offset");
                        return Ok(());
                    }
                } else if let Some(limit) = a {
                    if let Some(lim) = parse_int(limit) {
                        (0, Some(lim))
                    } else {
                        println!("Invalid argument: limit");
                        return Ok(());
                    }
                } else {
                    (0, None)
                };

                let limit = limit.unwrap_or(WORD_SIZE * VM_MAX_RAM as usize);
                let mem = match client
                    .cmd(&Command::ReadMemory {
                        start: offset,
                        len: limit,
                    })
                    .await?
                {
                    Response::ReadMemory(regs) => regs,
                    other => panic!("Unexpected response {:?}", other),
                };

                for (i, chunk) in mem.chunks(WORD_SIZE).enumerate() {
                    print!(" {:06x}:", offset + i * WORD_SIZE);
                    for byte in chunk {
                        print!(" {:02x}", byte);
                    }
                    println!("");
                }
            }
            other => {
                println!("Unknown command");
            }
        }
    }

    Ok(())
}

fn parse_int(s: &str) -> Option<usize> {
    let (s, radix) = if s.starts_with("0x") {
        (&s[2..], 16)
    } else {
        (s, 10)
    };

    let s = s.replace("_", "");

    usize::from_str_radix(&s, radix).ok()
}
