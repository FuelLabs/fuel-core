mod costs;
mod state;

use state::{
    extract_state,
    State,
};

use std::{
    collections::HashMap,
    io::{
        BufRead,
        BufWriter,
        Write,
    },
    path::PathBuf,
    sync::mpsc::{
        channel,
        TryRecvError,
    },
};

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
/// The format the output should be written to.
pub enum OutputFormat {
    Yaml,
    Json,
    Rust,
}

pub fn collect(
    baseline: String,
    readers: Vec<Box<dyn BufRead>>,
    mut output: PathBuf,
    debug: bool,
    format: OutputFormat,
    all: bool,
) {
    let mut state = State {
        all,
        ids: HashMap::new(),
        throughput: HashMap::new(),
        groups: HashMap::new(),
        baseline,
    };

    let (tx, rx) = channel();

    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

    let mut readers = readers.into_iter();
    let mut reader = readers.next().unwrap();

    let mut line = String::new();
    while let Err(TryRecvError::Empty) = rx.try_recv() {
        match reader.read_line(&mut line) {
            Ok(0) => {
                reader = match readers.next() {
                    Some(r) => r,
                    None => break,
                };
            }
            Ok(_) => (),
            Err(e) => {
                dbg!(e);
            }
        }

        if debug {
            eprintln!("{line}");
        }
        extract_state(&line, &mut state, debug);

        line.clear();
    }
    if debug {
        eprintln!("{state}");
    }
    if output.is_dir() {
        match format {
            OutputFormat::Yaml => output.push("gas-costs.yaml"),
            OutputFormat::Json => output.push("gas-costs.json"),
            OutputFormat::Rust => output.push("gas-costs.rs"),
        }
    }

    let file = std::fs::File::create(output.clone()).unwrap();
    let mut writer = BufWriter::new(file);

    match format {
        OutputFormat::Yaml => {
            serde_yaml::to_writer(writer, &state.to_yaml()).unwrap();
        }
        OutputFormat::Json => {
            serde_json::to_writer(writer, &state.to_json()).unwrap();
        }
        OutputFormat::Rust => write!(&mut writer, "{}", state.to_rust_code()).unwrap(),
    }

    println!("Successfully wrote output to {}", output.display());
}
