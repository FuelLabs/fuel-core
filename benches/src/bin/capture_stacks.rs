use std::{
    collections::HashMap,
    process::{
        Command,
        Stdio,
    },
};

static CONFIG: &str = include_str!("../../capture.yaml");

#[derive(Debug, Clone)]
struct Counts {
    count: usize,
    total: usize,
    percentage: f64,
    sanity: usize,
}

fn main() {
    let config: serde_yaml::Value = serde_yaml::from_str(CONFIG).unwrap();
    dbg!(&config);
    let bin = config.get("bin").unwrap().as_str().unwrap().to_string();
    let baseline = config
        .get("baseline")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let sanity_fn = config.get("sanity").unwrap().as_str().unwrap().to_string();
    let time = config.get("time").unwrap().as_u64().unwrap();
    let benches = config.get("benches").unwrap().as_sequence().unwrap();

    let mut found_baseline = None;
    let mut state = HashMap::new();

    for map in benches {
        let bench_name = map.get("bench").unwrap().as_str().unwrap().to_string();
        let fn_name = map.get("func").unwrap().as_str().unwrap().to_string();

        let count = counts(&bin, &bench_name, &fn_name, &sanity_fn, time);
        state.insert(bench_name.clone(), count.clone());
        if bench_name == baseline {
            found_baseline = Some(count);
        }
    }

    let found_baseline = found_baseline.unwrap();
    let mut counts: Vec<_> = state.into_iter().collect();
    counts.sort_by_key(|c| (c.1.percentage * 100.0) as i64);

    let mut out_list = Vec::with_capacity(counts.len() + 2);
    out_list.push(serde_yaml::from_str::<serde_yaml::Value>(CONFIG).unwrap());
    for (bench, count) in counts {
        let mut out_map = serde_yaml::Mapping::with_capacity(7);
        let count_percentage = count.percentage * 100.0;
        let count_times = (count.percentage / found_baseline.percentage) as usize;
        let count_sanity = (count.sanity as f64 / count.total as f64) * 100.0;
        println!(
            "{}: {} out of {} : {:.4}%, {}X noop, s: {} : {:.4}%",
            bench,
            count.count,
            count.total,
            count_percentage,
            count_times,
            count.sanity,
            count_sanity,
        );
        out_map.insert("bench".into(), bench.into());
        out_map.insert("count".into(), count.count.into());
        out_map.insert("total".into(), count.total.into());
        out_map.insert("percent".into(), count_percentage.into());
        out_map.insert("times_larger".into(), count_times.into());
        out_map.insert("sanity_fn".into(), count_sanity.into());
        out_map.insert("sanity_fn_total".into(), count.sanity.into());
        out_list.push(serde_yaml::Value::from(out_map));
    }

    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("result.yaml");
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)
        .unwrap();
    serde_yaml::to_writer(out, &out_list).unwrap();

    let out_stacks = std::env::current_dir().unwrap().join("out.stacks");
    let _ = std::fs::remove_file(out_stacks);
}

fn count(folded: Vec<u8>, fn_name: &str, sanity_fn: &str) -> Counts {
    let folded = String::from_utf8_lossy(&folded);

    let lines = folded.lines();
    let mut total = 0;
    let mut target = None;
    let mut sanity = 0;
    for line in lines {
        let start = line.rfind(' ').unwrap() + 1;
        let amount = line[start..].parse::<usize>().unwrap();
        if line.contains(fn_name) {
            match &mut target {
                Some(a) => *a += amount,
                None => target = Some(amount),
            }
        }
        if line.contains(sanity_fn) {
            sanity += amount;
        }
        total += amount;
    }
    let target = target.unwrap_or_default();
    let percentage = target as f64 / total as f64;
    Counts {
        count: target,
        total,
        percentage,
        sanity,
    }
}

#[cfg(target_os = "macos")]
fn counts(
    bin: &str,
    bench_name: &str,
    fn_name: &str,
    sanity_fn: &str,
    time: u64,
) -> Counts {
    Command::new("sudo")
        .arg("dtrace")
        .arg("-x")
        .arg("ustackframes=100")
        .arg("-n")
        .arg("profile-997 /pid == $target/ { @[ustack(100)] = count(); }")
        .arg("-o")
        .arg("out.stacks")
        .arg("-c")
        .arg(format!(
            "{} --bench ^{}$ --profile-time {}",
            bin, bench_name, time
        ))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .unwrap();

    let folded = Command::new("inferno-collapse-dtrace")
        .arg("out.stacks")
        .output()
        .unwrap();
    count(folded.stdout, fn_name, sanity_fn)
}

#[cfg(target_os = "linux")]
fn counts(
    bin: &str,
    bench_name: &str,
    fn_name: &str,
    sanity_fn: &str,
    time: u64,
) -> Counts {
    let mut cmd = Command::new("perf");
    cmd.arg("record")
        .arg("-F")
        .arg("997")
        .arg("--call-graph")
        .arg("dwarf")
        .arg("-g")
        .arg("--")
        .arg(bin)
        .arg("--bench")
        .arg(format!("^{}$", bench_name))
        .arg("--profile-time")
        .arg(time.to_string())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    cmd.output().unwrap();

    let mut script = Command::new("perf")
        .arg("script")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let script_out = script.stdout.take().unwrap();

    let folded = Command::new("inferno-collapse-perf")
        .stdin(script_out)
        .output()
        .unwrap();

    count(folded.stdout, fn_name, sanity_fn)
}
