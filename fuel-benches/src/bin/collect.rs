use clap::Parser;
use ctrlc;
use serde_json::Value;
use std::{
    collections::HashMap,
    fmt::Display,
    io::{
        BufRead,
        BufReader,
        BufWriter,
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

    /// Path to store output yaml. Defaults to current directory.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Print the input and output.
    #[arg(short, long)]
    debug: bool,
}

fn main() {
    let Args {
        baseline,
        output,
        debug,
    } = Args::parse();
    let mut output = output.unwrap_or_else(|| std::env::current_dir().unwrap());

    let (tx, rx) = channel();

    ctrlc::set_handler(move || tx.send(()).expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin);

    let mut line = String::new();
    let mut state = State {
        state: HashMap::new(),
        throughput: HashMap::new(),
        groups: HashMap::new(),
        baseline: baseline.unwrap_or_else(|| "alu/noop".to_string()),
    };
    while let Err(TryRecvError::Empty) = rx.try_recv() {
        let _ = reader.read_line(&mut line).unwrap();

        if debug {
            eprintln!("{}", line);
        }
        extract_state(&line, &mut state, debug);

        line.clear();
    }
    if debug {
        eprintln!("{}", state);
    }
    if output.is_dir() {
        output.push("gas-costs.yaml");
    }
    match state.state.get(&state.baseline).copied() {
        Some(baseline) => {
            let baseline: u64 = baseline.as_nanos().try_into().unwrap();
            let State {
                mut state,
                groups,
                throughput,
                ..
            } = state;
            let mut groups: Vec<_> = groups.into_iter().collect();
            groups.sort_unstable_by_key(|(k, _)| k.clone());
            let mut yaml = serde_yaml::Mapping::with_capacity(groups.len() + 1);
            for (group, mut benches) in groups.into_iter() {
                benches.sort_unstable();
                let mut benches_yaml = serde_yaml::Mapping::with_capacity(benches.len());
                for bench in benches {
                    if let Some(mean) = state.remove(&bench) {
                        let ratio = map_to_ratio(baseline, mean);
                        if debug {
                            eprintln!(
                                "id: {}, baseline: {}, mean: {:?}, ratio: {}",
                                bench, baseline, mean, ratio
                            );
                        }
                        benches_yaml.insert(
                            serde_yaml::to_value(bench).unwrap(),
                            serde_yaml::to_value(ratio).unwrap(),
                        );
                    }
                }
                if !benches_yaml.is_empty() {
                    yaml.insert(
                        serde_yaml::to_value(group).unwrap(),
                        serde_yaml::to_value(benches_yaml).unwrap(),
                    );
                }
            }
            if !state.is_empty() {
                let mut benches: Vec<_> = state.into_iter().collect();
                benches.sort_unstable_by_key(|(k, _)| k.clone());
                let benches_yaml: serde_yaml::Mapping = benches
                    .into_iter()
                    .map(|(bench, mean)| {
                        let ratio = map_to_ratio(baseline, mean);
                        if debug {
                            eprintln!(
                                "id: {}, baseline: {}, mean: {:?}, ratio: {}",
                                bench, baseline, mean, ratio
                            );
                        }
                        (
                            serde_yaml::to_value(bench).unwrap(),
                            serde_yaml::to_value(ratio).unwrap(),
                        )
                    })
                    .collect();
                if !benches_yaml.is_empty() {
                    yaml.insert(
                        serde_yaml::to_value("no-group".to_string()).unwrap(),
                        serde_yaml::to_value(benches_yaml).unwrap(),
                    );
                }
            }
            let slopes: Vec<_> = yaml
                .iter()
                .filter_map(|(group, benches)| {
                    let benches = benches.as_mapping()?;
                    let x: Vec<_> = benches
                        .keys()
                        .filter_map(|bench| throughput.get(bench.as_str()?))
                        .collect();
                    if x.len() != benches.len() {
                        return None
                    };

                    let avg_x = x.iter().copied().sum::<u64>() as f64 / x.len() as f64;
                    let avg_y = benches.values().filter_map(|i| i.as_u64()).sum::<u64>()
                        as f64
                        / benches.len() as f64;
                    let sum_x_y: f64 = x
                        .iter()
                        .copied()
                        .zip(benches.values().filter_map(|i| i.as_u64()))
                        .map(|(x, y)| (*x as f64 - avg_x) * (y as f64 - avg_y))
                        .sum();
                    let sq_x: f64 = x.iter().map(|i| (**i as f64 - avg_x).powi(2)).sum();
                    let slope = sum_x_y / sq_x;
                    Some((group.as_str()?.to_string(), slope))
                })
                .collect();
            for (group, slope) in slopes {
                if let Some(map) =
                    yaml.get_mut(group).and_then(|map| map.as_mapping_mut())
                {
                    map.insert(
                        serde_yaml::to_value("throughput-slope".to_string()).unwrap(),
                        serde_yaml::to_value(slope).unwrap(),
                    );
                }
            }
            let file = std::fs::File::create(output.clone()).unwrap();
            let writer = BufWriter::new(file);
            serde_yaml::to_writer(writer, &serde_yaml::Value::Mapping(yaml)).unwrap();

            println!("Successfully wrote output to {}", output.display());
        }
        None => eprintln!(
            "Could not produce output as baseline {} was not found in recording",
            state.baseline
        ),
    }
}

#[derive(Debug)]
enum Output {
    Mean(Mean),
    Group(Group),
}

#[derive(Debug)]
struct Mean {
    id: String,
    mean: Duration,
    throughput: Option<u64>,
}

#[derive(Debug)]
struct Group {
    id: String,
    benches: Vec<String>,
}

#[derive(Debug)]
struct State {
    baseline: String,
    state: HashMap<String, Duration>,
    throughput: HashMap<String, u64>,
    groups: HashMap<String, Vec<String>>,
}

fn extract_state(line: &str, state: &mut State, debug: bool) {
    match decode_input(&line) {
        Some(Output::Mean(Mean {
            id,
            mean,
            throughput,
        })) => {
            if debug {
                eprintln!("id: {}, mean: {:?}", id, mean);
            }
            state.state.insert(id.clone(), mean);
            if let Some(throughput) = throughput {
                state.throughput.insert(id, throughput);
            }
        }
        Some(Output::Group(Group { id, benches })) => {
            state.groups.entry(id).or_default().extend(benches);
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
        let iter = self.state.iter();
        match self.state.get(&self.baseline) {
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
                    writeln!(f, "ID: {}, Mean: {:?}", id, mean)?;
                }
            }
        }
        Ok(())
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
    fn handles_groups() {
        let input = r#"
        {"reason":"group-complete","group_name":"mem/mcli","benchmarks":["mem/mcli/10000","mem/mcli/100000"],"report_directory":""}
        {"reason":"benchmark-complete","id":"mem/mcp/10000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10000,"unit":"bytes"}], "mean":{"estimate":141.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        {"reason":"benchmark-complete","id":"mem/mcp/100000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10000,"unit":"bytes"}], "mean":{"estimate":141.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        {"reason":"group-complete","group_name":"mem/mcp","benchmarks":["mem/mcp/10000","mem/mcp/100000"],"report_directory":""}
        {"reason":"benchmark-complete","id":"mem/mcpi/10000","report_directory":"","iteration_count":[],"measured_values":[],"unit":"ns","throughput":[{"per_iteration":10000,"unit":"bytes"}], "mean":{"estimate":141.6387085050968,"lower_bound":141.05228097712418,"upper_bound":142.32943553585415,"unit":"ns"},"median":{"estimate":140.51177523784355,"lower_bound":140.39754464285716,"upper_bound":140.73739964179842,"unit":"ns"}}
        "#;

        let mut state = State {
            baseline: "".into(),
            state: Default::default(),
            throughput: Default::default(),
            groups: Default::default(),
        };
        for line in input.lines() {
            extract_state(line, &mut state, false);
        }

        assert_eq!(state.groups.len(), 2);
        assert_eq!(
            state.groups.get("mem/mcli").unwrap(),
            &["mem/mcli/10000", "mem/mcli/100000"]
        );
        assert_eq!(
            state.groups.get("mem/mcp").unwrap(),
            &["mem/mcp/10000", "mem/mcp/100000"]
        );
        assert_eq!(state.state.len(), 3);
        assert!(state.state.contains_key("mem/mcp/10000"));
        assert!(state.state.contains_key("mem/mcp/100000"));
        assert!(state.state.contains_key("mem/mcpi/10000"));
    }
}
