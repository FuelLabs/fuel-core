use clap::Parser;
use fuel_core_benches::{
    collect,
    collect::OutputFormat,
};
use std::{
    fs::File,
    io::{
        BufRead,
        BufReader,
    },
    path::PathBuf,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Benchmark id to use as the base line.
    #[arg(short, long)]
    baseline: Option<String>,

    /// Path to store output. Defaults to current directory.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Path to saved json for input. Defaults to stdin.
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Print the input and output.
    #[arg(short, long)]
    debug: bool,

    /// Include all sample values for dependent measurements.
    #[arg(short, long)]
    all: bool,

    /// The format the output should be written to.
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Yaml)]
    format: OutputFormat,
}

fn main() {
    let Args {
        baseline,
        input,
        output,
        debug,
        format,
        all,
    } = Args::parse();
    if all && matches!(format, OutputFormat::Rust) {
        panic!("The flag `all` cannot be used with {format:?}");
    }

    let baseline = baseline.unwrap_or_else(|| "noop/noop".to_string());

    let readers: Vec<Box<dyn BufRead>> = match input {
        Some(path) => {
            if path.is_dir() {
                std::fs::read_dir(path)
                    .unwrap()
                    .filter_map(|dir| {
                        let dir = dir.unwrap();
                        dir.file_type().unwrap().is_file().then(|| {
                            let r: Box<dyn BufRead> = Box::new(BufReader::new(
                                std::fs::File::open(dir.path()).unwrap(),
                            ));
                            r
                        })
                    })
                    .collect()
            } else {
                let reader = BufReader::new(File::open(path).unwrap());
                let reader = Box::new(reader);
                vec![reader]
            }
        }
        None => {
            let stdin = std::io::stdin();
            vec![Box::new(BufReader::new(stdin))]
        }
    };

    let output = output.unwrap_or_else(|| std::env::current_dir().unwrap());

    collect(baseline, readers, output, debug, format, all);
}
