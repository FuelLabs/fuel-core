use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context};
use graph::itertools::Itertools;
use graph::prelude::serde_json::{json, Value};
use graph::prelude::{reqwest, serde_json};

/// Parses stdout bytes into a prefixed String
pub fn pretty_output(blob: &[u8], prefix: &str) -> String {
    blob.split(|b| *b == b'\n')
        .map(String::from_utf8_lossy)
        .map(|line| format!("{}{}", prefix, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A file in the `tests` crate root
#[derive(Debug, Clone)]
pub struct TestFile {
    pub relative: String,
    pub path: PathBuf,
}

impl TestFile {
    /// Create a new file where `path` is taken relative to the `tests` crate root
    pub fn new(relative: &str) -> Self {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let path = cwd.join(relative);
        Self {
            relative: relative.to_string(),
            path,
        }
    }

    pub fn create(&self) -> File {
        std::fs::File::create(&self.path).unwrap()
    }

    pub fn read(&self) -> anyhow::Result<File> {
        std::fs::File::open(&self.path)
            .with_context(|| format!("Failed to open file {}", self.path.to_str().unwrap()))
    }

    pub fn reader(&self) -> anyhow::Result<BufReader<File>> {
        Ok(BufReader::new(self.read()?))
    }

    pub fn newer(&self, other: &TestFile) -> bool {
        self.path.metadata().unwrap().modified().unwrap()
            > other.path.metadata().unwrap().modified().unwrap()
    }

    pub fn append(&self, name: &str) -> Self {
        let mut path = self.path.clone();
        path.push(name);
        Self {
            relative: format!("{}/{}", self.relative, name),
            path,
        }
    }
}

impl std::fmt::Display for TestFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.relative)
    }
}

pub fn contains_subslice<T: PartialEq>(data: &[T], needle: &[T]) -> bool {
    data.windows(needle.len()).any(|w| w == needle)
}

/// Returns captured stdout
pub fn run_cmd(command: &mut Command) -> String {
    let program = command.get_program().to_str().unwrap().to_owned();
    let output = command
        .output()
        .context(format!("failed to run {}", program))
        .unwrap();
    println!(
        "stdout:\n{}",
        pretty_output(&output.stdout, &format!("[{}:stdout] ", program))
    );
    println!(
        "stderr:\n{}",
        pretty_output(&output.stderr, &format!("[{}:stderr] ", program))
    );

    String::from_utf8(output.stdout).unwrap()
}

/// Run a command, check that it succeeded and return its stdout and stderr
/// in a friendly error format for display
pub async fn run_checked(cmd: &mut tokio::process::Command) -> anyhow::Result<()> {
    let std_cmd = cmd.as_std();
    let cmdline = format!(
        "{} {}",
        std_cmd.get_program().to_str().unwrap(),
        std_cmd
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .join(" ")
    );
    let output = cmd
        .output()
        .await
        .with_context(|| format!("Command failed: {cmdline}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Command failed: {}\ncmdline: {cmdline}\nstdout: {stdout}\nstderr: {stderr}",
            output.status,
        )
    }
}

pub async fn graphql_query(endpoint: &str, query: &str) -> anyhow::Result<Value> {
    graphql_query_with_vars(endpoint, query, Value::Null).await
}

pub async fn graphql_query_with_vars(
    endpoint: &str,
    query: &str,
    vars: Value,
) -> anyhow::Result<Value> {
    let query = if vars == Value::Null {
        json!({ "query": query }).to_string()
    } else {
        json!({ "query": query, "variables": vars }).to_string()
    };
    let client = reqwest::Client::new();
    let res = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .body(query)
        .send()
        .await?;
    let text = res.text().await?;
    let body: Value = serde_json::from_str(&text)?;

    Ok(body)
}
