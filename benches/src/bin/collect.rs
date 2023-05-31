use clap::Parser;
use fuel_core_types::fuel_vm::GasCostsValues;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use std::{
    collections::{
        HashMap,
        HashSet,
    },
    fmt::Display,
    io::{
        BufRead,
        BufReader,
        BufWriter,
        Write,
    },
    path::PathBuf,
    sync::mpsc::{
        channel,
        TryRecvError,
    },
    time::Duration,
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

    /// Include all sample values for dependant measurements.
    #[arg(short, long)]
    all: bool,

    /// The format the output should be written to.
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Yaml)]
    format: OutputFormat,

    /// The style of benchmark.
    #[arg(long, value_enum, default_value_t = BenchType::GroupPerBench)]
    bench_type: BenchType,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
/// The format the output should be written to.
enum OutputFormat {
    Yaml,
    Json,
    Rust,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
/// The style of benchmark.
enum BenchType {
    GroupPerBench,
    SingleGroup,
}

#[derive(Debug)]
/// Output from a given measurement.
enum Output {
    /// The mean time of an op measurement.
    Mean(Mean),
    /// The ids linked to a group.
    Group(Group),
}

#[derive(Debug)]
/// The mean time of an op measurement.
struct Mean {
    /// The id of the measurement.
    id: String,
    /// The mean time of the measurement.
    mean: Duration,
    /// The throughput of the measurement
    /// if this measurement has one.
    throughput: Option<u64>,
}

#[derive(Debug)]
/// The ids linked to a group.
struct Group {
    /// The id of the group.
    id: String,
    /// The ids of the measurements in the group.
    benches: Vec<String>,
}

#[derive(Debug, Clone)]
/// The state of the collector.
struct State {
    /// The baseline to compare against.
    baseline: String,
    /// Should all measurements be included.
    all: bool,
    /// Map of ids to their mean times.
    ids: HashMap<String, Duration>,
    /// Map of ids to their throughput.
    throughput: HashMap<String, u64>,
    /// Map of groups to their ids.
    groups: HashMap<String, Vec<String>>,
    /// Type of benchmark.
    bench_type: BenchType,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Costs(HashMap<String, Cost>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum Cost {
    Relative(u64),
    Dependent {
        base: u64,
        dep_per_unit: u64,
    },
    DependentAll {
        samples: Vec<Sample>,
        dep_per_unit: u64,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
struct Sample {
    throughput: u64,
    time: u64,
}

fn main() {
    let Args {
        baseline,
        input,
        output,
        debug,
        format,
        all,
        bench_type,
    } = Args::parse();
    if all && matches!(format, OutputFormat::Rust) {
        panic!("The flag `all` cannot be used with {format:?}");
    }
    let mut output = output.unwrap_or_else(|| std::env::current_dir().unwrap());

    let (tx, rx) = channel();

    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

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
                vec![Box::new(BufReader::new(std::fs::File::open(path).unwrap()))]
            }
        }
        None => {
            let stdin = std::io::stdin();
            vec![Box::new(BufReader::new(stdin))]
        }
    };

    let mut line = String::new();
    let mut state = State {
        all,
        ids: HashMap::new(),
        throughput: HashMap::new(),
        groups: HashMap::new(),
        baseline: baseline.unwrap_or_else(|| "noop/noop".to_string()),
        bench_type,
    };
    let mut readers = readers.into_iter();
    let mut reader = readers.next().unwrap();
    while let Err(TryRecvError::Empty) = rx.try_recv() {
        match reader.read_line(&mut line) {
            Ok(amount) if amount == 0 => {
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

fn extract_state(line: &str, state: &mut State, debug: bool) {
    match decode_input(line) {
        Some(Output::Mean(Mean {
            id,
            mean,
            throughput,
        })) => {
            if debug {
                eprintln!("id: {id}, mean: {mean:?}");
            }
            state.ids.insert(id.clone(), mean);
            if let Some(throughput) = throughput {
                if debug {
                    eprintln!("throughput: {throughput}");
                }
                state.throughput.insert(id, throughput);
            }
        }
        Some(Output::Group(Group { id, benches })) => {
            state.groups.entry(id.clone()).or_default().extend(benches);
            eprintln!("{id} complete");
        }
        _ => (),
    }
}

fn decode_input(line: &str) -> Option<Output> {
    let val: Value = serde_json::from_str(line).ok()?;
    match val.get("reason")?.as_str()? {
        "benchmark-complete" => {
            let id = match val.get("id") {
                Some(Value::String(val)) => val.clone(),
                _ => return None,
            };
            let mean = val.get("mean")?;
            let mean = match mean.get("estimate") {
                Some(Value::Number(val)) => match mean.get("unit") {
                    Some(Value::String(unit)) => {
                        let val = val.as_f64().unwrap();
                        match unit.as_str() {
                            "ns" => std::time::Duration::from_nanos(val as u64),
                            "us" => std::time::Duration::from_micros(val as u64),
                            "ms" => std::time::Duration::from_millis(val as u64),
                            "s" => std::time::Duration::from_secs(val as u64),
                            _ => return None,
                        }
                    }
                    _ => return None,
                },
                _ => return None,
            };
            let throughput = if let Some(t) = val.get("throughput")?.as_array()?.get(0) {
                Some(t.as_object()?.get("per_iteration")?.as_u64()?)
            } else {
                None
            };
            Some(Output::Mean(Mean {
                id,
                mean,
                throughput,
            }))
        }
        "group-complete" => {
            let id = val.get("group_name")?.as_str()?.to_owned();
            let benches = val
                .get("benchmarks")?
                .as_array()?
                .iter()
                .filter_map(|v| Some(v.as_str()?.to_string()))
                .collect();
            Some(Output::Group(Group { id, benches }))
        }
        _ => None,
    }
}

fn map_to_ratio(baseline: u64, mean: Duration) -> u64 {
    let mean: u64 = mean.as_nanos().try_into().unwrap();
    mean.checked_div(baseline).unwrap_or(1).max(1)
}

impl Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let iter = self.ids.iter();
        match self.ids.get(&self.baseline) {
            Some(noop) => {
                for (id, mean) in iter {
                    writeln!(
                        f,
                        "ID: {}, Mean: {:?}, Diff {:?}",
                        id,
                        mean,
                        mean.saturating_sub(*noop)
                    )?;
                }
            }
            None => {
                for (id, mean) in iter {
                    writeln!(f, "ID: {id}, Mean: {mean:?}")?;
                }
            }
        }
        Ok(())
    }
}

const TEMPLATE: [&str; 5] = [
    r##"use super::*;
"##,
    r##"
pub const GIT: &str = ""##,
    r##"";"##,
    r##"
pub fn default_gas_costs() -> GasCostsValues {
    GasCostsValues {"##,
    r##"    }
}
"##,
];

impl State {
    fn to_rust_code(&self) -> String {
        let remaps = [
            ("mod", "mod_op"),
            ("move", "move_op"),
            ("ret_contract", "ret"),
            ("rvrt_contract", "rvrt"),
            ("retd_contract", "retd"),
        ];
        let removes = [("ret_script"), ("rvrt_script"), ("retd_script")]
            .into_iter()
            .collect::<HashSet<_>>();
        let mut value = serde_yaml::to_value(self.to_gas_costs()).unwrap();

        let mut yaml = self.to_yaml();

        let update_values = |values: &mut serde_yaml::Value| {
            let m = values.as_mapping_mut().unwrap();
            for k in &removes {
                m.remove(k);
            }
            for (old, new) in remaps {
                let v = m.remove(old);
                if let Some(v) = v {
                    m.insert(serde_yaml::Value::String(new.to_string()), v);
                }
            }
        };

        update_values(&mut value);
        update_values(&mut yaml);

        let have = yaml
            .as_mapping()
            .unwrap()
            .keys()
            .map(|k| k.as_str().unwrap())
            .collect::<HashSet<_>>();
        let all_keys = value
            .as_mapping()
            .unwrap()
            .keys()
            .map(|k| k.as_str().unwrap())
            .collect::<HashSet<_>>();

        let mut diff = have.symmetric_difference(&all_keys).collect::<Vec<_>>();
        diff.sort_unstable();

        if !diff.is_empty() {
            eprintln!("Warning the following keys were not set by this bench:\n{diff:?}\nWas this intentional?");
        }

        let map = value.as_mapping().unwrap();
        let indent = "        ";
        let inner = map
            .into_iter()
            .map(|(name, value)| {
                let name = name.as_str().unwrap();
                match value {
                    serde_yaml::Value::Number(i) => {
                        format!("\n{}{}: {},", indent, name, i.as_u64().unwrap())
                    }
                    serde_yaml::Value::Mapping(m) => {
                        format!(
                            "\n{}{}: DependentCost {{\n{}    base: {},\n{}    dep_per_unit: {},\n{}}},",
                            indent,
                            name,
                            indent,
                            m.get("base").unwrap().as_u64().unwrap(),
                            indent,
                            m.get("dep_per_unit").unwrap().as_u64().unwrap(),
                            indent,
                        )
                    }
                    _ => unreachable!(),
                }
            })
            .collect::<String>();
        let git_output = std::process::Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .output()
            .unwrap();
        let git_string = String::from_utf8_lossy(&git_output.stdout);
        let generated_by = format!(
            "/// File generated by fuel-core: {}:{}. With the following git hash",
            file!(),
            line!()
        );
        format!(
            "{}{}{}{}{}{}{}\n{}",
            TEMPLATE[0],
            generated_by,
            TEMPLATE[1],
            git_string.trim(),
            TEMPLATE[2],
            TEMPLATE[3],
            inner,
            TEMPLATE[4]
        )
    }

    fn to_gas_costs(&self) -> GasCostsValues {
        serde_yaml::from_value(self.to_yaml()).unwrap()
    }

    fn to_yaml(&self) -> serde_yaml::Value {
        let v = serde_yaml::to_value(self.clone().into_costs()).unwrap();
        let v = match v {
            serde_yaml::Value::Mapping(m) => m,
            _ => unreachable!(),
        };
        let mut v = v.into_iter().collect::<Vec<_>>();
        v.sort_unstable_by_key(|(k, _)| k.as_str().unwrap().to_string());
        v.into_iter().collect::<serde_yaml::Mapping>().into()
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self.clone().into_costs()).unwrap()
    }

    fn into_costs(self) -> Costs {
        let (state, costs) = self.into_dependant_costs();
        state.into_relative_costs(costs)
    }

    fn get_baseline(&self) -> u64 {
        self.ids
            .get(&self.baseline)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "Could not produce output as baseline {} was not found in recording.\n{:?}",
                    self.baseline, self.ids
                )
            })
            .as_nanos()
            .try_into()
            .unwrap()
    }

    fn into_dependant_costs(self) -> (Self, Costs) {
        let baseline = self.get_baseline();
        let State {
            all,
            mut ids,
            mut groups,
            throughput,
            baseline: baseline_name,
            bench_type,
        } = self;
        let dependant_groups = dependant_groups(baseline, &groups, &throughput, &mut ids);
        let mut costs = Costs::with_capacity(dependant_groups.len());

        if all {
            let iter = dependant_groups.into_iter().map(
                |DependentGroup {
                     group: name,
                     points,
                 }| {
                    groups.remove(&name.0);
                    let samples = points
                        .iter()
                        .map(|Point { throughput, ratio }| Sample {
                            throughput: *throughput,
                            time: *ratio,
                        })
                        .collect();
                    let dep_per_unit = (1.0 / slope(points)) as u64;
                    (
                        name.0,
                        Cost::DependentAll {
                            samples,
                            dep_per_unit,
                        },
                    )
                },
            );
            costs.0.extend(iter);
        } else {
            let iter = dependant_groups.into_iter().map(
                |DependentGroup {
                     group: name,
                     points,
                 }| {
                    groups.remove(&name.0);
                    let base = points.first().unwrap().ratio;
                    let dep_per_unit = (1.0 / slope(points)) as u64;
                    (name.0, Cost::Dependent { base, dep_per_unit })
                },
            );
            costs.0.extend(iter);
        }
        (
            Self {
                all,
                baseline: baseline_name,
                ids,
                throughput,
                groups,
                bench_type,
            },
            costs,
        )
    }

    fn into_relative_costs(self, mut costs: Costs) -> Costs {
        let baseline = self.get_baseline();
        let State {
            mut ids, groups, ..
        } = self;

        let relative = groups.into_iter().filter_map(|(name, samples)| {
            let relative = samples
                .into_iter()
                .find(|sample| sample.ends_with(&name))
                .and_then(|sample| ids.remove(&sample))
                .map(|mean| Cost::Relative(map_to_ratio(baseline, mean)))?;
            Some((name, relative))
        });
        costs.0.extend(relative);
        costs
    }
}

impl Costs {
    fn with_capacity(size: usize) -> Self {
        Self(HashMap::with_capacity(size))
    }
}

fn slope(x_y: Vec<Point>) -> f64 {
    let avg_x = x_y
        .iter()
        .map(|Point { throughput: x, .. }| x)
        .copied()
        .sum::<u64>() as f64
        / x_y.len() as f64;
    let avg_y = x_y
        .iter()
        .map(|Point { ratio: y, .. }| y)
        .copied()
        .sum::<u64>() as f64
        / x_y.len() as f64;
    let sum_x_y: f64 = x_y
        .iter()
        .map(
            |Point {
                 throughput: x,
                 ratio: y,
             }| (*x as f64 - avg_x) * (*y as f64 - avg_y),
        )
        .sum();
    let sq_x: f64 = x_y
        .iter()
        .map(|Point { throughput: x, .. }| (*x as f64 - avg_x).powi(2))
        .sum();
    sum_x_y / sq_x
}

fn dependant_groups(
    baseline: u64,
    groups: &HashMap<String, Vec<String>>,
    throughput: &HashMap<String, u64>,
    ids: &mut HashMap<String, Duration>,
) -> Vec<DependentGroup> {
    let groups = groups
        .iter()
        .map(|(name, points)| {
            (
                GroupName(name.clone()),
                points.iter().cloned().map(Id::from).collect(),
            )
        })
        .collect::<HashMap<_, _>>();
    let throughput = throughput
        .iter()
        .map(|(id, throughput)| (Id::from(id.clone()), *throughput))
        .collect();
    let ids_inner = ids
        .iter()
        .map(|(id, mean)| (Id::from(id.clone()), *mean))
        .collect();
    let r = if groups.len() == 1 {
        let (name, points) = groups.into_iter().next().unwrap();
        let r =
            dependant_groups_single_group(baseline, name, points, throughput, ids_inner);
        DependentGroups {
            groups: vec![r.groups],
            to_remove: r.to_remove,
        }
    } else {
        dependant_groups_per_bench(baseline, groups, throughput, ids_inner)
    };
    ids.retain(|id, _| !r.to_remove.contains(id));
    r.groups
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct GroupName(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct BenchName(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct GroupBenchAmount {
    name: GroupName,
    bench: BenchName,
    amount: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct BenchAmount {
    bench: BenchName,
    amount: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Id {
    GroupBenchAmount(GroupBenchAmount),
    BenchAmount(BenchAmount),
    Unknown(String),
}

impl From<String> for Id {
    fn from(value: String) -> Self {
        let mut split = value.split('/');
        let first = split.next();
        let second = split.next();
        let third = split.next();
        match (first, second, third) {
            (Some(_), None, _) | (None, _, _) => Self::Unknown(value),
            (Some(bench), Some(amount), None) => match amount.parse::<usize>() {
                Ok(amount) => Self::BenchAmount(BenchAmount {
                    bench: BenchName(bench.to_string()),
                    amount,
                }),
                Err(_) => Self::Unknown(value),
            },
            (Some(group), Some(bench), Some(amount)) => match amount.parse::<usize>() {
                Ok(amount) => Self::GroupBenchAmount(GroupBenchAmount {
                    name: GroupName(group.to_string()),
                    bench: BenchName(bench.to_string()),
                    amount,
                }),
                Err(_) => Self::Unknown(value),
            },
        }
    }
}

impl From<Id> for String {
    fn from(value: Id) -> Self {
        match value {
            Id::GroupBenchAmount(GroupBenchAmount {
                name,
                bench,
                amount,
            }) => format!("{}/{}/{}", name.0, bench.0, amount),
            Id::BenchAmount(BenchAmount { bench, amount }) => {
                format!("{}/{}", bench.0, amount)
            }
            Id::Unknown(s) => s,
        }
    }
}

struct DependentGroups<T> {
    groups: T,
    to_remove: HashSet<String>,
}
struct DependentGroup {
    group: GroupName,
    points: Vec<Point>,
}

struct Point {
    throughput: u64,
    ratio: u64,
}

fn dependant_groups_per_bench(
    baseline: u64,
    groups: HashMap<GroupName, Vec<Id>>,
    throughput: HashMap<Id, u64>,
    mut ids: HashMap<Id, Duration>,
) -> DependentGroups<Vec<DependentGroup>> {
    let mut to_remove = HashSet::new();
    let groups = groups
        .iter()
        .filter(|(_, samples)| {
            samples.iter().any(|sample| throughput.contains_key(sample))
        })
        .map(|(name, samples)| {
            let mut samples = samples
                .iter()
                .filter_map(|sample| {
                    let (k, v) = ids.remove_entry(sample)?;
                    to_remove.insert(String::from(k));
                    Some((v, throughput.get(sample).copied()?))
                })
                .map(|(mean, t)| Point {
                    throughput: t,
                    ratio: map_to_ratio(baseline, mean),
                })
                .collect::<Vec<_>>();
            samples.sort_unstable_by_key(|point| point.ratio);
            DependentGroup {
                group: name.clone(),
                points: samples,
            }
        })
        .collect::<Vec<_>>();
    DependentGroups { groups, to_remove }
}

fn dependant_groups_single_group(
    baseline: u64,
    group: GroupName,
    points: Vec<Id>,
    mut throughput: HashMap<Id, u64>,
    mut ids: HashMap<Id, Duration>,
) -> DependentGroups<DependentGroup> {
    let mut to_remove = HashSet::new();
    let mut points = points
        .iter()
        .filter_map(|id| {
            let t = throughput.remove(id)?;
            let (k, v) = ids.remove_entry(id)?;
            to_remove.insert(String::from(k));
            Some(Point {
                throughput: t,
                ratio: map_to_ratio(baseline, v),
            })
        })
        .collect::<Vec<_>>();
    points.sort_unstable_by_key(|point| point.ratio);
    DependentGroups {
        groups: DependentGroup { group, points },
        to_remove,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependant_groups_per_bench() {
        let groups = [
            ("a", vec!["a/1", "a/2", "a/3"]),
            ("b", vec!["b/1", "b/2", "b/3"]),
            ("c", vec!["c/1", "c/2", "c/3"]),
            ("d", vec!["d"]),
        ]
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                v.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();

        let throughput = [
            ("a/1", 1),
            ("a/2", 2),
            ("a/3", 3),
            ("b/1", 4),
            ("b/2", 5),
            ("b/3", 6),
            ("c/1", 7),
            ("c/2", 8),
            ("c/3", 9),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect::<HashMap<_, _>>();

        let mut ids = [
            ("a/1", Duration::from_nanos(1)),
            ("a/2", Duration::from_nanos(2)),
            ("a/3", Duration::from_nanos(3)),
            ("b/1", Duration::from_nanos(4)),
            ("b/2", Duration::from_nanos(5)),
            ("b/3", Duration::from_nanos(6)),
            ("c/1", Duration::from_nanos(7)),
            ("c/2", Duration::from_nanos(8)),
            ("c/3", Duration::from_nanos(9)),
            ("d", Duration::from_nanos(10)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect::<HashMap<_, _>>();
        let mut r = dependant_groups_per_bench(100, &groups, &throughput, &mut ids);
        r.sort_by_key(|(k, _)| k.clone());

        let expected = vec![
            ("a".to_string(), vec![(1, 1), (2, 1), (3, 1)]),
            ("b".to_string(), vec![(4, 1), (5, 1), (6, 1)]),
            ("c".to_string(), vec![(7, 1), (8, 1), (9, 1)]),
        ];
        assert_eq!(expected, r);

        assert_eq!(ids.len(), 1);
        assert!(ids.contains_key("d"));
    }

    #[test]
    fn test_dependant_groups_single_group() {
        let groups = [(
            "a",
            vec![
                "a/a/1", "a/a/2", "a/a/3", "a/b/1", "a/b/2", "a/b/3", "a/c/1", "a/c/2",
                "a/c/3", "a/d",
            ],
        )]
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                v.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();

        let throughput = [
            ("a/a/1", 1),
            ("a/a/2", 2),
            ("a/a/3", 3),
            ("a/b/1", 4),
            ("a/b/2", 5),
            ("a/b/3", 6),
            ("a/c/1", 7),
            ("a/c/2", 8),
            ("a/c/3", 9),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect::<HashMap<_, _>>();

        let mut ids = [
            ("a/a/1", Duration::from_nanos(1)),
            ("a/a/2", Duration::from_nanos(2)),
            ("a/a/3", Duration::from_nanos(3)),
            ("a/b/1", Duration::from_nanos(4)),
            ("a/b/2", Duration::from_nanos(5)),
            ("a/b/3", Duration::from_nanos(6)),
            ("a/c/1", Duration::from_nanos(7)),
            ("a/c/2", Duration::from_nanos(8)),
            ("a/c/3", Duration::from_nanos(9)),
            ("a/d", Duration::from_nanos(10)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect::<HashMap<_, _>>();
        let mut r = dependant_groups_single_group(100, &groups, &throughput, &mut ids);
        r.sort_by_key(|(k, _)| k.clone());

        let expected = vec![
            ("a".to_string(), vec![(1, 1), (2, 1), (3, 1)]),
            ("b".to_string(), vec![(4, 1), (5, 1), (6, 1)]),
            ("c".to_string(), vec![(7, 1), (8, 1), (9, 1)]),
        ];
        assert_eq!(expected, r);

        assert_eq!(ids.len(), 1);
        assert!(ids.contains_key("a/d"));
    }

    #[test]
    fn handles_json() {
        let input = r#"
        {"reason":"benchmark-complete","id":"alu/add","report_directory":"/Users/freesig/fuel/fuel-core/target/criterion/reports/alu/add","iteration_count":[2,4,6,8,10,12,14,16,18,20,22,24,26,28,30,32,34,36,38,40,42,44,46,48,50,52,54,56,58,60,62,64,66,68,70,72,74,76,78,80,82,84,86,88,90,92,94,96,98,100,102,104,106,108,110,112,114,116,118,120,122,124,126,128,130,132,134,136,138,140,142,144,146,148,150,152,154,156,158,160,162,164,166,168,170,172,174,176,178,180,182,184,186,188,190,192,194,196,198,200],"measured_values":[1916.0,1499.0,1539.0,1622.0,2704.0,2624.0,2583.0,3083.0,3374.0,3914.0,3663.0,3831.0,4123.0,4497.0,4664.0,6331.0,7498.0,8247.0,6538.0,7497.0,6373.0,6998.0,7165.0,7747.0,7956.0,8206.0,8207.0,8497.0,8957.0,9621.0,11163.0,10621.0,10455.0,12707.0,11746.0,12330.0,11914.0,12122.0,12829.0,12704.0,15204.0,14164.0,15330.0,17038.0,15080.0,14245.0,14621.0,15706.0,16870.0,16163.0,16498.0,18914.0,18457.0,17996.0,16623.0,17331.0,18995.0,26664.0,18331.0,18539.0,19622.0,19622.0,21371.0,21329.0,20122.0,20747.0,21538.0,21164.0,41038.0,25038.0,23871.0,21748.0,25582.0,25413.0,24498.0,24497.0,24704.0,25330.0,29038.0,33539.0,48413.0,43997.0,34331.0,27580.0,27746.0,26912.0,30581.0,29497.0,29039.0,36747.0,29540.0,30748.0,29913.0,30705.0,35539.0,35956.0,32039.0,32914.0,31829.0,43330.0],"unit":"ns","throughput":[],"typical":{"estimate":177.42910299985223,"lower_bound":169.82270041534676,"upper_bound":186.11014872166245,"unit":"ns"},"mean":{"estimate":186.5770081898676,"lower_bound":173.70739633513531,"upper_bound":206.03206457957904,"unit":"ns"},"median":{"estimate":166.56140350877195,"lower_bound":162.7917797888386,"upper_bound":168.76984126984127,"unit":"ns"},"median_abs_dev":{"estimate":13.302762628550237,"lower_bound":8.179643444525901,"upper_bound":18.60727600768299,"unit":"ns"},"slope":{"estimate":177.42910299985223,"lower_bound":169.82270041534676,"upper_bound":186.11014872166245,"unit":"ns"},"change":{"mean":{"estimate":-0.057396520935410256,"lower_bound":-0.17800428256454653,"upper_bound":0.07505289453108437,"unit":"%"},"median":{"estimate":-0.06570032828885364,"lower_bound":-0.1076552495911749,"upper_bound":-0.03368699426837496,"unit":"%"},"change":"NoChange"}}
        "#;
        decode_input(input).unwrap();
    }

    #[test]
    fn test_ratio() {
        let baseline = Duration::from_nanos(19);
        let mean = Duration::from_nanos(200);
        let baseline: u64 = baseline.as_nanos().try_into().unwrap();
        let ratio = map_to_ratio(baseline, mean);
        dbg!(ratio);
    }

    #[test]
    fn handles_groups() {
        let input = r#"
        {"reason":"group-complete","group_name":"mcli","benchmarks":["mcli/10000","mcli/100000"],"report_directory":""}
        {"reason":"benchmark-complete","id":"mcp/10000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10000,"unit":"bytes"}], "mean":{"estimate":141.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        {"reason":"benchmark-complete","id":"mcp/100000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10000,"unit":"bytes"}], "mean":{"estimate":141.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        {"reason":"group-complete","group_name":"mcp","benchmarks":["mcp/10000","mcp/100000"],"report_directory":""}
        {"reason":"benchmark-complete","id":"mcpi/10000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10000,"unit":"bytes"}], "mean":{"estimate":141.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        "#;

        let mut state = State {
            all: true,
            baseline: "".into(),
            ids: Default::default(),
            throughput: Default::default(),
            groups: Default::default(),
            bench_type: BenchType::GroupPerBench,
        };
        for line in input.lines() {
            extract_state(line, &mut state, false);
        }

        assert_eq!(state.groups.len(), 2);
        assert_eq!(
            state.groups.get("mcli").unwrap(),
            &["mcli/10000", "mcli/100000"]
        );
        assert_eq!(
            state.groups.get("mcp").unwrap(),
            &["mcp/10000", "mcp/100000"]
        );
        assert_eq!(state.ids.len(), 3);
        assert!(state.ids.contains_key("mcp/10000"));
        assert!(state.ids.contains_key("mcp/100000"));
        assert!(state.ids.contains_key("mcpi/10000"));
    }

    #[test]
    fn handles_single_group() {
        let input = r#"
        {"reason":"benchmark-complete","id":"transaction/baseline","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[],"typical":{"estimate":1889882.1811999998,"lower_bound":1818207.9852500001,"upper_bound":1959960.6525499995,"unit":"ns"},"mean":{"estimate":1889882.1811999998,"lower_bound":1818207.9852500001,"upper_bound":1959960.6525499995,"unit":"ns"},"median":{"estimate":2066936.6800000002,"lower_bound":1921769.18,"upper_bound":2129712.48,"unit":"ns"},"median_abs_dev":{"estimate":268832.4105752703,"lower_bound":148023.99710404882,"upper_bound":490886.3605170251,"unit":"ns"},"slope":null,"change":null}
        {"reason":"benchmark-complete","id":"transaction/coin signed to void/1","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":1,"unit":"elements"}],"typical":{"estimate":1815368.467300133,"lower_bound":1740368.7886516275,"upper_bound":1884789.912018716,"unit":"ns"},"mean":{"estimate":1788408.4250238338,"lower_bound":1721285.165525651,"upper_bound":1855536.4136125282,"unit":"ns"},"median":{"estimate":1789624.7424242424,"lower_bound":1626135.7962962964,"upper_bound":1924139.7551177638,"unit":"ns"},"median_abs_dev":{"estimate":493475.1727353033,"lower_bound":358187.4059985071,"upper_bound":552965.3663889918,"unit":"ns"},"slope":{"estimate":1815368.467300133,"lower_bound":1740368.7886516275,"upper_bound":1884789.912018716,"unit":"ns"},"change":null}
        {"reason":"benchmark-complete","id":"transaction/coin signed to void/5","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":5,"unit":"elements"}],"typical":{"estimate":1945030.0660000006,"lower_bound":1870768.0224399988,"upper_bound":2020571.2344,"unit":"ns"},"mean":{"estimate":1945030.0660000006,"lower_bound":1870768.0224399988,"upper_bound":2020571.2344,"unit":"ns"},"median":{"estimate":1912134.18,"lower_bound":1702750.0,"upper_bound":2112053.34,"unit":"ns"},"median_abs_dev":{"estimate":586745.1665031909,"lower_bound":317708.77982354147,"upper_bound":613535.6887235641,"unit":"ns"},"slope":null,"change":{"mean":{"estimate":-0.11130252605452717,"lower_bound":-0.15345271554152712,"upper_bound":-0.06627552411181012,"unit":"%"},"median":{"estimate":-0.17885249664369063,"lower_bound":-0.26979844063444025,"upper_bound":-0.06516287176747959,"unit":"%"},"change":"Improved"}}
        {"reason":"benchmark-complete","id":"transaction/coin signed to void/10","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10,"unit":"elements"}],"typical":{"estimate":2078746.3885714284,"lower_bound":2008188.1525833332,"upper_bound":2150255.779511904,"unit":"ns"},"mean":{"estimate":2078746.3885714284,"lower_bound":2008188.1525833332,"upper_bound":2150255.779511904,"unit":"ns"},"median":{"estimate":1933027.7619047621,"lower_bound":1892492.0476190476,"upper_bound":1994475.1904761903,"unit":"ns"},"median_abs_dev":{"estimate":340981.82654636283,"lower_bound":250126.93525935948,"upper_bound":439032.6500631122,"unit":"ns"},"slope":null,"change":null}
        {"reason":"benchmark-complete","id":"transaction/coin signed to coin/1","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":1,"unit":"elements"}],"typical":{"estimate":1715275.5789655682,"lower_bound":1630389.4998739497,"upper_bound":1803179.2416809876,"unit":"ns"},"mean":{"estimate":1822676.2677465812,"lower_bound":1754526.5679907592,"upper_bound":1891016.9501783117,"unit":"ns"},"median":{"estimate":1861245.7530364373,"lower_bound":1687251.4808006536,"upper_bound":1934184.401010101,"unit":"ns"},"median_abs_dev":{"estimate":459261.88326260424,"lower_bound":375964.2268631938,"upper_bound":547895.8363325717,"unit":"ns"},"slope":{"estimate":1715275.5789655682,"lower_bound":1630389.4998739497,"upper_bound":1803179.2416809876,"unit":"ns"},"change":null}
        {"reason":"benchmark-complete","id":"transaction/coin signed to coin/5","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":5,"unit":"elements"}],"typical":{"estimate":2112173.198374464,"lower_bound":2025266.614548286,"upper_bound":2194655.319425682,"unit":"ns"},"mean":{"estimate":2047806.468793442,"lower_bound":1992160.1500447006,"upper_bound":2103341.206058957,"unit":"ns"},"median":{"estimate":2070557.5576923077,"lower_bound":1899493.2432432433,"upper_bound":2158100.286989796,"unit":"ns"},"median_abs_dev":{"estimate":394637.9425061692,"lower_bound":292749.4158759545,"upper_bound":456072.6309018425,"unit":"ns"},"slope":{"estimate":2112173.198374464,"lower_bound":2025266.614548286,"upper_bound":2194655.319425682,"unit":"ns"},"change":{"mean":{"estimate":0.0038542126696146095,"lower_bound":-0.03832507669133906,"upper_bound":0.050946665147317965,"unit":"%"},"median":{"estimate":-0.06560498662418723,"lower_bound":-0.15186094251108695,"upper_bound":0.07417369706093924,"unit":"%"},"change":"NoChange"}}
        {"reason":"benchmark-complete","id":"transaction/coin signed to coin/10","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10,"unit":"elements"}],"typical":{"estimate":2268644.436500001,"lower_bound":2199260.4559625,"upper_bound":2337557.3593749995,"unit":"ns"},"mean":{"estimate":2268644.436500001,"lower_bound":2199260.4559625,"upper_bound":2337557.3593749995,"unit":"ns"},"median":{"estimate":2389008.325,"lower_bound":2045390.65,"upper_bound":2534425.0,"unit":"ns"},"median_abs_dev":{"estimate":444798.52460324764,"lower_bound":211916.4548648561,"upper_bound":543187.565356493,"unit":"ns"},"slope":null,"change":null}
        {"reason":"benchmark-complete","id":"transaction/coin signed to variable/1","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":1,"unit":"elements"}],"typical":{"estimate":1827290.8180789123,"lower_bound":1737261.718098797,"upper_bound":1919613.5518274526,"unit":"ns"},"mean":{"estimate":1835533.6676709047,"lower_bound":1762910.7480887289,"upper_bound":1907239.9732616614,"unit":"ns"},"median":{"estimate":1898789.15390788,"lower_bound":1710463.7352941176,"upper_bound":2057497.423076923,"unit":"ns"},"median_abs_dev":{"estimate":500956.2864531229,"lower_bound":318430.9275211464,"upper_bound":600240.4658365166,"unit":"ns"},"slope":{"estimate":1827290.8180789123,"lower_bound":1737261.718098797,"upper_bound":1919613.5518274526,"unit":"ns"},"change":null}
        {"reason":"benchmark-complete","id":"transaction/coin signed to variable/5","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":5,"unit":"elements"}],"typical":{"estimate":2230432.666000001,"lower_bound":2161926.8147399994,"upper_bound":2294048.26688,"unit":"ns"},"mean":{"estimate":2230432.666000001,"lower_bound":2161926.8147399994,"upper_bound":2294048.26688,"unit":"ns"},"median":{"estimate":2380120.82,"lower_bound":2353895.02,"upper_bound":2399023.32,"unit":"ns"},"median_abs_dev":{"estimate":114422.14373660112,"lower_bound":76964.25540161158,"upper_bound":146538.93601441337,"unit":"ns"},"slope":null,"change":{"mean":{"estimate":0.09202742140418252,"lower_bound":0.047145999823460356,"upper_bound":0.14263077425730597,"unit":"%"},"median":{"estimate":0.0880668413623209,"lower_bound":0.05185550935699168,"upper_bound":0.15976667110378706,"unit":"%"},"change":"Regressed"}}
        {"reason":"benchmark-complete","id":"transaction/coin signed to variable/10","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10,"unit":"elements"}],"typical":{"estimate":2251959.3466666667,"lower_bound":2179328.338428572,"upper_bound":2323748.72520238,"unit":"ns"},"mean":{"estimate":2251959.3466666667,"lower_bound":2179328.338428572,"upper_bound":2323748.72520238,"unit":"ns"},"median":{"estimate":2424176.595238095,"lower_bound":2100154.761904762,"upper_bound":2508753.9523809524,"unit":"ns"},"median_abs_dev":{"estimate":347228.4438354627,"lower_bound":214612.27658987045,"upper_bound":554929.2629480363,"unit":"ns"},"slope":null,"change":null}
        {"reason":"benchmark-complete","id":"transaction/coin signed to change/1","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":1,"unit":"elements"}],"typical":{"estimate":1890630.3881483672,"lower_bound":1801184.4360116583,"upper_bound":1979463.850204347,"unit":"ns"},"mean":{"estimate":1833890.9853527676,"lower_bound":1767215.9022395543,"upper_bound":1899715.8732464188,"unit":"ns"},"median":{"estimate":1991915.0875129714,"lower_bound":1727566.5747126436,"upper_bound":2085342.5434362935,"unit":"ns"},"median_abs_dev":{"estimate":317634.5534822032,"lower_bound":166552.23858016325,"upper_bound":525027.1081551042,"unit":"ns"},"slope":{"estimate":1890630.3881483672,"lower_bound":1801184.4360116583,"upper_bound":1979463.850204347,"unit":"ns"},"change":null}
        {"reason":"benchmark-complete","id":"transaction/coin signed to change/5","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":5,"unit":"elements"}],"typical":{"estimate":2140521.3000886654,"lower_bound":2056204.7793729955,"upper_bound":2218018.0995483277,"unit":"ns"},"mean":{"estimate":1984522.8747400963,"lower_bound":1912220.0898092734,"upper_bound":2055971.3315018201,"unit":"ns"},"median":{"estimate":1972176.2434017595,"lower_bound":1816333.5224358975,"upper_bound":2187047.097826087,"unit":"ns"},"median_abs_dev":{"estimate":585768.5758240585,"lower_bound":354195.39814420295,"upper_bound":604825.9826807033,"unit":"ns"},"slope":{"estimate":2140521.3000886654,"lower_bound":2056204.7793729955,"upper_bound":2218018.0995483277,"unit":"ns"},"change":{"mean":{"estimate":-0.04412969729330696,"lower_bound":-0.08991868471407319,"upper_bound":0.0017407129383119422,"unit":"%"},"median":{"estimate":-0.12408269135778993,"lower_bound":-0.1929563985537539,"upper_bound":-0.0066164051749735735,"unit":"%"},"change":"NoChange"}}
        {"reason":"benchmark-complete","id":"transaction/coin signed to change/10","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10,"unit":"elements"}],"typical":{"estimate":2291747.2304999996,"lower_bound":2213472.13595,"upper_bound":2370334.831225,"unit":"ns"},"mean":{"estimate":2291747.2304999996,"lower_bound":2213472.13595,"upper_bound":2370334.831225,"unit":"ns"},"median":{"estimate":2330570.85,"lower_bound":2017628.125,"upper_bound":2590872.9,"unit":"ns"},"median_abs_dev":{"estimate":598587.3967279493,"lower_bound":270542.06332191825,"upper_bound":621084.3316635486,"unit":"ns"},"slope":null,"change":null}
        {"reason":"group-complete","group_name":"transaction","benchmarks":["transaction/baseline","transaction/coin signed to void/1","transaction/coin signed to void/5","transaction/coin signed to void/10","transaction/coin signed to coin/1","transaction/coin signed to coin/5","transaction/coin signed to coin/10","transaction/coin signed to variable/1","transaction/coin signed to variable/5","transaction/coin signed to variable/10","transaction/coin signed to change/1","transaction/coin signed to change/5","transaction/coin signed to change/10"],"report_directory":"/Users/freesig/fuel/fuel-core/target/criterion/reports/transaction"}
        "#;

        let mut state = State {
            all: true,
            baseline: "transaction/baseline".into(),
            ids: Default::default(),
            throughput: Default::default(),
            groups: Default::default(),
            bench_type: BenchType::SingleGroup,
        };
        for line in input.lines() {
            extract_state(line, &mut state, false);
        }

        assert_eq!(state.groups.len(), 1);
        assert_eq!(
            state.groups.get("transaction").unwrap(),
            &[
                "transaction/baseline",
                "transaction/coin signed to void/1",
                "transaction/coin signed to void/5",
                "transaction/coin signed to void/10",
                "transaction/coin signed to coin/1",
                "transaction/coin signed to coin/5",
                "transaction/coin signed to coin/10",
                "transaction/coin signed to variable/1",
                "transaction/coin signed to variable/5",
                "transaction/coin signed to variable/10",
                "transaction/coin signed to change/1",
                "transaction/coin signed to change/5",
                "transaction/coin signed to change/10",
            ]
        );
        assert_eq!(state.ids.len(), 13);
        assert!(state.ids.contains_key("transaction/baseline"));
        assert!(state.ids.contains_key("transaction/coin signed to void/1"));
        assert!(state.ids.contains_key("transaction/coin signed to void/5"));
        assert!(state.ids.contains_key("transaction/coin signed to void/10"));
        assert!(state.ids.contains_key("transaction/coin signed to coin/1"));
        assert!(state.ids.contains_key("transaction/coin signed to coin/5"));
        assert!(state.ids.contains_key("transaction/coin signed to coin/10"));
        assert!(state
            .ids
            .contains_key("transaction/coin signed to variable/1"));
        assert!(state
            .ids
            .contains_key("transaction/coin signed to variable/5"));
        assert!(state
            .ids
            .contains_key("transaction/coin signed to variable/10"));
        assert!(state
            .ids
            .contains_key("transaction/coin signed to change/1"));
        assert!(state
            .ids
            .contains_key("transaction/coin signed to change/5"));
        assert!(state
            .ids
            .contains_key("transaction/coin signed to change/10"));

        assert_eq!(state.throughput.len(), 12);
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to void/1"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to void/5"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to void/10"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to coin/1"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to coin/5"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to coin/10"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to variable/1"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to variable/5"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to variable/10"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to change/1"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to change/5"));
        assert!(state
            .throughput
            .contains_key("transaction/coin signed to change/10"));

        let (state, costs) = state.into_dependant_costs();
        assert_eq!(costs.0.len(), 4);
    }

    #[test]
    fn serialize_gas_costs() {
        let input = r#"
        {"reason":"benchmark-complete","id":"noop","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[],"mean":{"estimate":14.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        {"reason":"group-complete","group_name":"noop","benchmarks":["noop"],"report_directory":""}
        {"reason":"benchmark-complete","id":"add","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[],"mean":{"estimate":35.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        {"reason":"group-complete","group_name":"add","benchmarks":["add"],"report_directory":""}
        {"reason":"group-complete","group_name":"mcli","benchmarks":["mcli/10000","mcli/100000"],"report_directory":""}
        {"reason":"benchmark-complete","id":"mcp/10000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10000,"unit":"bytes"}], "mean":{"estimate":141.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        {"reason":"benchmark-complete","id":"mcp/100000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":100000,"unit":"bytes"}], "mean":{"estimate":1.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"us"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        {"reason":"group-complete","group_name":"mcp","benchmarks":["mcp/10000","mcp/100000"],"report_directory":""}
        {"reason":"benchmark-complete","id":"mcpi/10000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10000,"unit":"bytes"}], "mean":{"estimate":141.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        "#;

        let mut state = State {
            all: false,
            baseline: "noop".into(),
            ids: Default::default(),
            throughput: Default::default(),
            groups: Default::default(),
            bench_type: BenchType::GroupPerBench,
        };
        for line in input.lines() {
            extract_state(line, &mut state, false);
        }

        let costs = state.to_gas_costs();

        let ser = serde_yaml::to_string(&costs).unwrap();
        eprintln!("{ser}");
        eprintln!("{}", state.to_rust_code());
        let path = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches-outputs/src/test_gas_costs_output.rs"
        ));
        let manifest_path = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/benches-outputs/Cargo.toml"
        ));
        std::fs::write(path, state.to_rust_code()).unwrap();
        let output = std::process::Command::new("cargo")
            .arg("check")
            .arg("--manifest-path")
            .arg(&manifest_path)
            .output()
            .unwrap();
        println!("{}", String::from_utf8(output.stderr).unwrap());
        assert!(output.status.success());
    }

    #[test]
    fn test_slope() {
        assert_eq!(slope(vec![(4, 2), (8, 3), (12, 4), (16, 5)]), 0.25);
    }
}
