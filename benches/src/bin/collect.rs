use clap::Parser;
use fuel_core_types::fuel_tx::{
    consensus_parameters::gas::GasCostsValuesV4,
    ConsensusParameters,
    GasCosts,
};
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

    /// Include all sample values for dependent measurements.
    #[arg(short, long)]
    all: bool,

    /// The format the output should be written to.
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Yaml)]
    format: OutputFormat,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
/// The format the output should be written to.
enum OutputFormat {
    Yaml,
    Json,
    Rust,
    ConsensusParameters,
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
}

impl Default for State {
    fn default() -> Self {
        State {
            all: false,
            ids: HashMap::new(),
            throughput: HashMap::new(),
            groups: HashMap::new(),
            baseline: "noop/noop".to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Costs(HashMap<String, Cost>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DependentCost {
    LightOperation { base: u64, units_per_gas: u64 },
    HeavyOperation { base: u64, gas_per_unit: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cost {
    Relative(u64),
    Dependent(DependentCost),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    };
    let mut readers = readers.into_iter();
    let mut reader = readers.next().unwrap();
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
            OutputFormat::ConsensusParameters => output.push("consensus-parameters.rs"),
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
        OutputFormat::ConsensusParameters => {
            let gas_costs = GasCosts::new(state.to_gas_costs().into());
            let mut consensus_parameters = ConsensusParameters::default();
            consensus_parameters.set_gas_costs(gas_costs);

            serde_json::to_writer_pretty(writer, &consensus_parameters).unwrap();
        }
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
            let throughput = if let Some(t) = val.get("throughput")?.as_array()?.first() {
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
    (mean + (baseline - 1))
        .checked_div(baseline)
        .unwrap_or(1)
        .max(1)
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
    r#"
pub const GIT: &str = ""#,
    r#"";"#,
    r##"
pub fn default_gas_costs() -> GasCostsValues {
    GasCostsValuesV4 {"##,
    r##"    }.into()
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
                    serde_yaml::Value::Tagged(m) => {
                        let tag = &m.tag;
                        let m = &m.value;
                        if tag.to_string().contains("HeavyOperation") {
                            let output = format!(
                                r"
        {}: DependentCost::HeavyOperation {{
             base: {},
             gas_per_unit: {},
         }},",
                                name,
                                m.get("base").unwrap().as_u64().unwrap(),
                                m.get("gas_per_unit").unwrap().as_u64().unwrap(),
                            );
                            output
                        } else {
                            let output = format!(
                                r"
        {}: DependentCost::LightOperation {{
             base: {},
             units_per_gas: {},
         }},",
                                name,
                                m.get("base").unwrap().as_u64().unwrap(),
                                m.get("units_per_gas").unwrap().as_u64().unwrap(),
                            );
                            output
                        }
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

    fn to_gas_costs(&self) -> GasCostsValuesV4 {
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
        let (state, costs) = self.into_dependent_costs();
        state.into_relative_costs(costs)
    }

    fn get_baseline(&self) -> u64 {
        self.ids
            .get(&self.baseline)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "Could not produce output as baseline {} was not found in recording",
                    self.baseline
                )
            })
            .as_nanos()
            .try_into()
            .unwrap()
    }

    fn into_dependent_costs(self) -> (Self, Costs) {
        let baseline = self.get_baseline();
        let State {
            all,
            mut ids,
            mut groups,
            throughput,
            baseline: baseline_name,
        } = self;

        let mut costs = Costs::with_capacity(groups.len());
        let dependent_groups = groups
            .iter()
            .filter(|(_, samples)| {
                samples.iter().any(|sample| throughput.contains_key(sample))
            })
            .map(|(name, samples)| {
                let mut samples = samples
                    .iter()
                    .filter_map(|sample| {
                        Some((ids.remove(sample)?, throughput.get(sample).copied()?))
                    })
                    .map(|(mean, t)| (t, map_to_ratio(baseline, mean)))
                    .collect::<Vec<_>>();
                samples.sort_unstable_by_key(|(_, mean)| *mean);
                (name.clone(), samples)
            })
            .collect::<Vec<_>>();

        let iter = dependent_groups.into_iter().map(|(name, x_y)| {
            groups.remove(&name);

            let cost = Cost::Dependent(dependent_cost(&name, x_y));
            (name, cost)
        });
        costs.0.extend(iter);

        (
            Self {
                all,
                baseline: baseline_name,
                ids,
                throughput,
                groups,
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
                .find(|sample| sample.starts_with(&name) && ids.contains_key(sample))
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

fn linear_regression(x_y: Vec<(u64, u64)>) -> f64 {
    let avg_x =
        x_y.iter().map(|(x, _)| x).copied().sum::<u64>() as f64 / x_y.len() as f64;
    let avg_y =
        x_y.iter().map(|(_, y)| y).copied().sum::<u64>() as f64 / x_y.len() as f64;
    let sum_x_y: f64 = x_y
        .iter()
        .map(|(x, y)| (*x as f64 - avg_x) * (*y as f64 - avg_y))
        .sum();
    let sq_x: f64 = x_y.iter().map(|(x, _)| (*x as f64 - avg_x).powi(2)).sum();

    // The original formula says:
    //
    // y = B * x,
    // where B = sum((x_i - x_avg) * (y_i - y_avg)) / sum((x_i - x_avg) ^ 2) = sum_x_y / sq_x
    //
    // We want to know how many elements fit into one `noop` opcode timing,
    // so we need value of `x / y`:
    //
    // y = B * x
    //     |
    //    \|/
    // 1 / B = x / y
    //     |
    //    \|/
    // 1 / sum_x_y / sq_x = x / y
    //     |
    //    \|/
    // sq_x / sum_x_y = x / y
    sq_x / sum_x_y
}

fn dependent_cost(name: &String, x_y: Vec<(u64, u64)>) -> DependentCost {
    const NEAR_LINEAR: f64 = 0.1;

    #[derive(PartialEq, Eq)]
    enum Type {
        /// The points have a linear property. The first point
        /// and the last points are almost the same(The difference is < 0.1).
        Linear,
        /// When the delta of the last point is much lower than
        /// the first point, it is a logarithmic chart.
        Logarithm,
        /// When the delta of the last point is much more than
        /// the first point, it is an exponential chart.
        Exp,
    }

    #[derive(Debug, Clone, Copy)]
    struct Point {
        /// Number of elements for the opcode.
        x: u64,
        /// Time in `noop`s required to process `x` elements.
        ///
        /// Note: If the time to process `noop` opcode is `20` nanoseconds
        /// and the time required to process `x` elements is `140`,
        /// the `y` is 7 in this case.
        y: u64,
    }

    impl Point {
        /// The price in `noop` time to process a single element.
        fn price(&self) -> f64 {
            (self.y as f64) / (self.x as f64)
        }

        /// The amount that this operation costs per increase in one `noop` time.
        fn amount(&self) -> f64 {
            (self.x as f64) / (self.y as f64)
        }
    }

    let linear_regression = linear_regression(x_y.clone());

    let x_y = x_y
        .into_iter()
        .map(|(x, y)| Point { x, y })
        .collect::<Vec<_>>();

    let first = x_y.first().unwrap();
    let last = x_y.last().unwrap();

    let near_linear = linear_regression * NEAR_LINEAR;
    let expected_type = if (linear_regression - first.amount()).abs() < near_linear
        && (linear_regression - last.amount()).abs() < near_linear
    {
        Type::Linear
    } else if first.price() > last.price() {
        Type::Logarithm
    } else {
        Type::Exp
    };

    let (base, amount): (u64, f64) = match expected_type {
        Type::Linear => {
            // The time of the first point is a base.
            // The minimal charge per element is the worse scenario
            // and we use it as an amount per unit.
            let mut x_y = x_y.into_iter();
            let base = x_y.next().unwrap().y;

            let amount = x_y
                .map(|p| p.amount())
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();
            (base, amount)
        }
        Type::Logarithm | Type::Exp => {
            if expected_type == Type::Exp {
                println!(
                    "The {} is exponential. We don't support exponential chart. \
                The opcode should be limited with upper bound. \n {:?}",
                    name, x_y
                );
            }

            // The logarithm function slows down fast, and the point where it becomes more
            // linear is the base point. After this point we use linear strategy.
            let last = x_y.last().unwrap().amount();
            let mut x_y = x_y.into_iter();

            let mut base = x_y.next().unwrap();

            let amount = x_y
                .skip_while(|p| {
                    if p.amount() < linear_regression - base.amount() {
                        base = *p;
                        true
                    } else {
                        false
                    }
                })
                .map(|p| p.amount())
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(last);
            (base.y, amount)
        }
    };

    if amount > 1.0 {
        DependentCost::LightOperation {
            base,
            units_per_gas: amount as u64,
        }
    } else {
        DependentCost::HeavyOperation {
            base,
            gas_per_unit: (1.0 / amount) as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn state_into_relative_costs_matches_samples_to_groups() {
        let input = r#"
        {"reason":"benchmark-complete","id":"noop/noop","report_directory":"/fuel-core/target/criterion/reports/noop/noop","iteration_count":[20579,41158,61737,82316,102895,123474,144053,164632,185211,205790,226369,246948,267527,288106,308685,329264,349843,370422,391001,411580,432159,452738,473317,493896,514475,535054,555633,576212,596791,617370,637949,658528,679107,699686,720265,740844,761423,782002,802581,823160,843739,864318,884897,905476,926055,946634,967213,987792,1008371,1028950,1049529,1070108,1090687,1111266,1131845,1152424,1173003,1193582,1214161,1234740,1255319,1275898,1296477,1317056,1337635,1358214,1378793,1399372,1419951,1440530,1461109,1481688,1502267,1522846,1543425,1564004,1584583,1605162,1625741,1646320,1666899,1687478,1708057,1728636,1749215,1769794,1790373,1810952,1831531,1852110,1872689,1893268,1913847,1934426,1955005,1975584,1996163,2016742,2037321,2057900],"measured_values":[389282.0,770797.0,1188427.0,1550880.0,1941738.0,2338649.0,2689515.0,3102895.0,3493736.0,3843974.0,4345608.0,4790617.0,5032116.0,5425160.0,5782422.0,6153689.0,6527003.0,6929596.0,7311469.0,7752961.0,8096879.0,8440853.0,8842395.0,9257219.0,10058182.0,10224299.0,10949755.0,11828899.0,12883105.0,13683938.0,16413343.0,13627041.0,13087408.0,13304575.0,13535697.0,13883527.0,15506137.0,14755226.0,15066220.0,15405773.0,15805531.0,16349976.0,16704086.0,16977259.0,17408869.0,17807203.0,18342606.0,18449738.0,18872388.0,19338438.0,19700298.0,20244544.0,20463792.0,21100051.0,21257107.0,21702514.0,22155040.0,22306333.0,22956387.0,23142080.0,23621411.0,23980328.0,24399532.0,24637273.0,25046541.0,25815672.0,25814418.0,26145303.0,26599206.0,26974627.0,28274114.0,27701356.0,28189035.0,28535485.0,28911095.0,29433053.0,29861502.0,30290108.0,30611717.0,30947687.0,31334219.0,31575650.0,32031711.0,32432008.0,32871005.0,33160229.0,33498200.0,34082473.0,34816058.0,34897933.0,35246469.0,35780041.0,36011687.0,36196850.0,36776318.0,36994151.0,37516038.0,37769430.0,38243417.0,38656250.0],"unit":"ns","throughput":[],"typical":{"estimate":18.857469852901115,"lower_bound":18.80991755561779,"upper_bound":18.923223760418672,"unit":"ns"},"mean":{"estimate":19.018693172864804,"lower_bound":18.874689771701124,"upper_bound":19.208667956348382,"unit":"ns"},"median":{"estimate":18.7980051843273,"lower_bound":18.77140984193862,"upper_bound":18.820593660892023,"unit":"ns"},"median_abs_dev":{"estimate":0.10730922897391375,"lower_bound":0.0812325582132701,"upper_bound":0.14041547076097582,"unit":"ns"},"slope":{"estimate":18.857469852901115,"lower_bound":18.80991755561779,"upper_bound":18.923223760418672,"unit":"ns"},"change":{"mean":{"estimate":-0.022826223844162996,"lower_bound":-0.04175747264497742,"upper_bound":-0.005980222776105934,"unit":"%"},"median":{"estimate":-0.009987775466800741,"lower_bound":-0.01278779831658372,"upper_bound":-0.007773882142105615,"unit":"%"},"change":"NoChange"}}
        {"reason":"group-complete","group_name":"noop","benchmarks":["noop/noop"],"report_directory":"/fuel-core/target/criterion/reports/noop"}
        {"reason":"benchmark-complete","id":"contract_root/test_b","report_directory":"/Volumes/Fuel/projects/FuelLabs/fuel-core/target/criterion/reports/contract_root/test_b","iteration_count":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"measured_values":[55969125.0,56143708.0,56070875.0,56446083.0,55284458.0,55575958.0,55562292.0,54611625.0,55169583.0,55756625.0,55473416.0,55187959.0,54961625.0,55105083.0,55144916.0,55879209.0,54749250.0,54745459.0,54731458.0,54612375.0,55069250.0,55551750.0,54944375.0,55457792.0,55276000.0,55580375.0,54754208.0,55046875.0,55313875.0,54756083.0,55104791.0,54734125.0,54853125.0,55141584.0,55071209.0,54997375.0,54770375.0,55104833.0,54911917.0,54924125.0,54923625.0,54617167.0,55281834.0,54639000.0,55284417.0,56533209.0,56687417.0,55747375.0,54843125.0,54792791.0,54881208.0,54618250.0,55585291.0,54643708.0,55165458.0,55089000.0,55331458.0,55029542.0,54938083.0,54980625.0,55133833.0,55131542.0,55607208.0,54999250.0,55162292.0,55075375.0,55621375.0,54975500.0,54729125.0,55682291.0,55181375.0,55166666.0,54747334.0,54719250.0,55561667.0,54923250.0,55619666.0,55118875.0,55347000.0,55339917.0,54804708.0,55304083.0,54754625.0,55773583.0,55411666.0,55134166.0,54958375.0,54873583.0,55044291.0,55179500.0,55161417.0,55459583.0,54935708.0,55161416.0,54972083.0,55446542.0,54918250.0,54687041.0,55506125.0,54776708.0],"unit":"ns","throughput":[],"typical":{"estimate":55182639.51,"lower_bound":55102229.70249999,"upper_bound":55267996.49725,"unit":"ns"},"mean":{"estimate":55182639.51,"lower_bound":55102229.70249999,"upper_bound":55267996.49725,"unit":"ns"},"median":{"estimate":55111979.0,"lower_bound":55022125.0,"upper_bound":55165458.0,"unit":"ns"},"median_abs_dev":{"estimate":350944.01586949825,"lower_bound":269307.6135188341,"upper_bound":506401.31334207935,"unit":"ns"},"slope":null,"change":null}
        {"reason":"benchmark-complete","id":"contract_root/test_c","report_directory":"/Volumes/Fuel/projects/FuelLabs/fuel-core/target/criterion/reports/contract_root/test_c","iteration_count":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"measured_values":[54881541.0,55167084.0,55069750.0,55097041.0,55038333.0,55082500.0,54983958.0,54867250.0,55324833.0,54954416.0,55007291.0,55436500.0,54699584.0,55743417.0,54767292.0,55069209.0,54756042.0,54812709.0,55476000.0,54692625.0,55201208.0,55067791.0,54872000.0,55866125.0,54966417.0,54780667.0,54879917.0,55085041.0,54680958.0,54851583.0,54747666.0,54879250.0,55303542.0,54986042.0,54796667.0,54376459.0,54536625.0,54535958.0,56011875.0,54531959.0,54355875.0,55293542.0,54396125.0,54930666.0,54472875.0,55089583.0,54641125.0,54417334.0,54606917.0,54407667.0,54895959.0,55105709.0,54491625.0,54695458.0,54567083.0,54873125.0,54544542.0,64412958.0,56095209.0,54300250.0,55058000.0,54617417.0,55095542.0,55600000.0,56537375.0,71663708.0,59887709.0,58745792.0,62623500.0,60810750.0,66510625.0,61822250.0,57435917.0,57221042.0,56591250.0,56781291.0,56302583.0,55933875.0,55793875.0,55166250.0,55924958.0,55800042.0,56612125.0,59934583.0,57355042.0,56426750.0,55144458.0,55987041.0,54776708.0,54834458.0,55490625.0,55277583.0,55151208.0,55024000.0,54754500.0,55358791.0,54915375.0,55410334.0,54941166.0,55122375.0],"unit":"ns","throughput":[],"typical":{"estimate":55889196.25,"lower_bound":55433887.1785,"upper_bound":56437883.57525,"unit":"ns"},"mean":{"estimate":55889196.25,"lower_bound":55433887.1785,"upper_bound":56437883.57525,"unit":"ns"},"median":{"estimate":55069479.5,"lower_bound":54953791.5,"upper_bound":55151208.0,"unit":"ns"},"median_abs_dev":{"estimate":546275.8211016655,"lower_bound":363514.981046319,"upper_bound":765701.3585060835,"unit":"ns"},"slope":null,"change":null}
        {"reason":"group-complete","group_name":"contract_root","benchmarks":["contract_root/test_a","contract_root/test_b","contract_root/test_c"],"report_directory":"/Volumes/Fuel/projects/FuelLabs/fuel-core/target/criterion/reports/contract_root"}
        "#;

        let mut state = State::default();
        for line in input.lines() {
            extract_state(line, &mut state, false);
        }
        let costs = Costs::default();
        let costs = state.into_relative_costs(costs);

        assert!(costs.0.contains_key("noop"));
        assert!(costs.0.contains_key("contract_root"));
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
    fn test_linear_regression() {
        assert_eq!(
            linear_regression(vec![(4, 2), (8, 3), (12, 4), (16, 5)]),
            4.0
        );
    }
}
