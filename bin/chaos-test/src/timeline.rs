use std::{
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::fault::FaultAction;

#[derive(Debug, Clone)]
pub enum Violation {
    Fork {
        height: u32,
        first_block_id: String,
        second_block_id: String,
        first_node: usize,
        second_node: usize,
    },
    HeightRegression {
        node: usize,
        previous_height: u32,
        new_height: u32,
    },
    GapDetected {
        node: usize,
        max_global_height: u32,
        node_height: u32,
        behind_for: Duration,
    },
    ConcurrentLeaders {
        height: u32,
        nodes: Vec<usize>,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Violation::Fork {
                height,
                first_block_id,
                second_block_id,
                first_node,
                second_node,
            } => {
                write!(
                    f,
                    "FORK at height {height}: node {first_node} has {first_block_id}, \
                     node {second_node} has {second_block_id}"
                )
            }
            Violation::HeightRegression {
                node,
                previous_height,
                new_height,
            } => {
                write!(
                    f,
                    "HEIGHT REGRESSION on node {node}: went from {previous_height} to {new_height}"
                )
            }
            Violation::GapDetected {
                node,
                max_global_height,
                node_height,
                behind_for,
            } => {
                write!(
                    f,
                    "GAP on node {node}: at height {node_height}, global max is \
                     {max_global_height}, behind for {behind_for:?}"
                )
            }
            Violation::ConcurrentLeaders { height, nodes } => {
                write!(
                    f,
                    "CONCURRENT LEADERS at height {height}: nodes {nodes:?}"
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum TimelineEventKind {
    FaultInjected(FaultAction),
    FaultRecovered(FaultAction),
    BlockProduced {
        node: usize,
        height: u32,
        local: bool,
    },
    Violation(Violation),
    Info(String),
}

impl fmt::Display for TimelineEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimelineEventKind::FaultInjected(action) => {
                write!(f, "FAULT INJECTED: {action}")
            }
            TimelineEventKind::FaultRecovered(action) => {
                write!(f, "FAULT RECOVERED: {action}")
            }
            TimelineEventKind::BlockProduced {
                node,
                height,
                local,
            } => {
                let source = if *local { "local" } else { "network" };
                write!(f, "BLOCK node={node} height={height} source={source}")
            }
            TimelineEventKind::Violation(v) => {
                write!(f, "VIOLATION: {v}")
            }
            TimelineEventKind::Info(msg) => {
                write!(f, "INFO: {msg}")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub elapsed: Duration,
    pub kind: TimelineEventKind,
}

#[derive(Clone)]
pub struct Timeline {
    start: Instant,
    events: Arc<Mutex<Vec<TimelineEvent>>>,
    seed: u64,
}

impl Timeline {
    pub fn new(seed: u64) -> Self {
        Self {
            start: Instant::now(),
            events: Arc::new(Mutex::new(Vec::new())),
            seed,
        }
    }

    pub fn record(&self, kind: TimelineEventKind) {
        let elapsed = self.start.elapsed();
        let event = TimelineEvent { elapsed, kind };
        self.events.lock().unwrap().push(event);
    }

    pub fn violations(&self) -> Vec<Violation> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| {
                if let TimelineEventKind::Violation(v) = &e.kind {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn print_report(&self) {
        let events = self.events.lock().unwrap();
        println!("\n{}", "=".repeat(72));
        println!("  CHAOS TEST REPORT");
        println!("  Seed: {}", self.seed);
        println!("  Duration: {:?}", self.start.elapsed());
        println!("  Total events: {}", events.len());
        println!("{}\n", "=".repeat(72));

        let violations: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, TimelineEventKind::Violation(_)))
            .collect();

        if violations.is_empty() {
            println!("  RESULT: PASS — no invariant violations detected\n");
        } else {
            println!(
                "  RESULT: FAIL — {} violation(s) detected\n",
                violations.len()
            );
            for v in &violations {
                println!("  [{:>8.2?}] {}", v.elapsed, v.kind);
            }
            println!();
        }

        // Print fault summary
        let faults: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, TimelineEventKind::FaultInjected(_)))
            .collect();
        let blocks: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, TimelineEventKind::BlockProduced { .. }))
            .collect();

        println!("  Summary:");
        println!("    Faults injected: {}", faults.len());
        println!("    Blocks observed: {}", blocks.len());
        println!("    Violations: {}", violations.len());
        println!("{}", "=".repeat(72));
    }
}
