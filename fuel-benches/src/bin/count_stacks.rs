use std::io::BufRead;

fn main() {
    let mut stacks = std::env::current_dir().unwrap();
    let mut args = std::env::args().skip(1);
    stacks.push(args.next().expect("Needs input file name"));
    let fn_name = args.next();

    let stacks = std::fs::File::open(stacks).unwrap();
    let reader = std::io::BufReader::new(stacks);
    let mut lines = reader.lines().map(Result::unwrap);
    let mut total = 0;
    let mut target = None;
    while let Some(line) = lines.next() {
        let start = line.rfind(' ').unwrap() + 1;
        let amount = line[start..].parse::<usize>().unwrap();
        if let Some(fn_name) = &fn_name {
            if line.contains(fn_name) {
                assert!(target.is_none(), "Match target function name more then once");
                target = Some(amount);
            }
        }
        total += amount;
    }
    match target {
        Some(target) => {
            let p = target as f64 / total as f64 * 100.0;
            println!("Total: {}, Target {} {:.2}%", total, target, p);
        }
        None => {
            println!("Total: {}", total);
        }
    }
}
