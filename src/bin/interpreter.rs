use clap::{App, Arg};
use fuel_asm::Opcode;
use json::{array, object};
use tracing::{info, trace};
use tracing_subscriber::fmt::Subscriber;

use std::io::{self, Read};

use fuel_vm_rust::prelude::*;

const NAME: &'static str = env!("CARGO_PKG_NAME");
const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const AUTHORS: &'static str = env!("CARGO_PKG_AUTHORS");

fn main() -> io::Result<()> {
    let matches = App::new(NAME)
        .version(VERSION)
        .author(AUTHORS)
        .arg(
            Arg::with_name("log-level")
                .short("l")
                .long("log-level")
                .value_name("LOG")
                .possible_values(&["error", "warn", "info", "debug", "trace"])
                .default_value("info")
                .takes_value(true)
                .help("Output log level"),
        )
        .arg(
            Arg::with_name("dummy-tx")
                .short("d")
                .long("dummy-tx")
                .value_name("DUMMY_TX")
                .takes_value(false)
                .required(false)
                .help("Use a dummy transaction instead of parsing from STDIN"),
        )
        .get_matches();

    let log = match matches.value_of("log-level").expect("Failed parsing log-level arg") {
        "error" => tracing::Level::ERROR,
        "warn" => tracing::Level::WARN,
        "info" => tracing::Level::INFO,
        "debug" => tracing::Level::DEBUG,
        "trace" => tracing::Level::TRACE,
        _ => unreachable!(),
    };

    let subscriber = Subscriber::builder().with_max_level(log).finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to register tracing subscriber!");

    let mut input_buffer = Vec::new();
    let n = io::stdin().lock().read_to_end(&mut input_buffer)?;
    trace!("Received {} bytes...", n);

    let mut input = input_buffer.as_slice();

    let dummy_tx = matches.is_present("dummy-tx");
    let tx = if dummy_tx {
        info!("Using dummy transaction...");
        Transaction::default()
    } else {
        let (n, tx) = Transaction::try_from_bytes(input)?;
        info!("Transaction parsed with {} bytes consumed...", n);
        input = &input[n..];
        tx
    };

    let mut interpreter = Interpreter::default();
    interpreter.init(tx).expect("Failed to initialize VM");

    info!("Fuel VM Interpreter {} initialized", VERSION);

    for opcode in input.chunks_exact(Opcode::BYTES_SIZE) {
        let opcode = Opcode::from_bytes_unchecked(opcode);

        trace!("{:?} parsed", opcode);

        interpreter.execute(opcode).expect("Instruction failed!");
    }

    info!("Execution finished.");

    let mut output = array![];
    for log in interpreter.log() {
        let obj = match log {
            LogEvent::Register { pc, register, value } => object! {
                log: "register", program_counter: *pc, register: *register, value: *value
            },

            LogEvent::Return { register, value } => object! {
                log: "return", register: *register, value: *value
            },
        };

        output.push(obj).expect("Failed to append log to JSON!");
    }

    println!("{}", output.dump());

    Ok(())
}
