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
    let time = config.get("time").unwrap().as_u64().unwrap();
    let benches = config.get("benches").unwrap().as_sequence().unwrap();

    let mut found_baseline = None;
    let mut state = HashMap::new();

    for map in benches {
        let bench_name = map.get("bench").unwrap().as_str().unwrap().to_string();
        let fn_name = map.get("func").unwrap().as_str().unwrap().to_string();
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
        // let mut out = std::env::current_dir().unwrap();
        // let bench_name = bench_name.replace("/", "-");
        // out.push(format!("folded-{}.stacks", bench_name));
        // dbg!(&out);
        // std::fs::write(out, folded.stdout).unwrap();
        let count = count(folded.stdout, fn_name);
        state.insert(bench_name.clone(), count.clone());
        if bench_name == baseline {
            found_baseline = Some(count);
        }
    }

    let found_baseline = found_baseline.unwrap();
    let mut counts: Vec<_> = state.into_iter().collect();
    counts.sort_by_key(|c| (c.1.percentage * 100.0) as i64);
    for (bench, count) in counts {
        println!(
            "{}: {} out of {} : {:.4}%, {}X noop",
            bench,
            count.count,
            count.total,
            count.percentage * 100.0,
            (count.percentage / found_baseline.percentage) as usize
        );
    }

    let out_stacks = std::env::current_dir().unwrap().join("out.stacks");
    let _ = std::fs::remove_file(out_stacks);
}

fn count(folded: Vec<u8>, fn_name: String) -> Counts {
    let folded = String::from_utf8_lossy(&folded);

    let mut lines = folded.lines();
    let mut total = 0;
    let mut target = None;
    while let Some(line) = lines.next() {
        let start = line.rfind(' ').unwrap() + 1;
        let amount = line[start..].parse::<usize>().unwrap();
        if line.contains(&fn_name) {
            assert!(
                target.is_none(),
                "Match target function name more then once"
            );
            target = Some(amount);
        }
        total += amount;
    }
    let target = target.unwrap();
    let percentage = target as f64 / total as f64;
    Counts {
        count: target,
        total,
        percentage,
    }
}
