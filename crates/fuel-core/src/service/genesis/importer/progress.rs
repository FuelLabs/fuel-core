use std::io::IsTerminal;

use indicatif::{
    MultiProgress,
    ProgressBar,
    ProgressDrawTarget,
    ProgressStyle,
};

#[derive(Clone)]
pub struct ProgressReporter {
    bar: ProgressBar,
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new(Target::Logs, usize::MAX)
    }
}

pub enum Target {
    Cli,
    Logs,
}

impl ProgressReporter {
    pub fn new(target: Target, max: usize) -> Self {
        let max = u64::try_from(max).unwrap_or(u64::MAX);
        let target = match target {
            Target::Cli => ProgressDrawTarget::stderr(),
            Target::Logs => ProgressDrawTarget::hidden(),
        };
        let is_hidden = target.is_hidden();

        let bar = ProgressBar::with_draw_target(Some(max), target);
        if !is_hidden {
            let style = ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg} {eta}",
            )
            .unwrap();

            bar.set_style(style);
        }

        ProgressReporter { bar }
    }

    pub fn new_detect_output(max: usize) -> Self {
        let target = if std::io::stderr().is_terminal() && !cfg!(test) {
            Target::Cli
        } else {
            Target::Logs
        };
        Self::new(target, max)
    }

    pub fn set_progress(&self, progress: u64) {
        self.bar.set_position(progress);
        if self.bar.is_hidden() {
            if let Some(len) = self.bar.length() {
                let human_eta = self.eta();
                tracing::info!("Processing: {progress}/{len}. ({human_eta})");
            } else {
                tracing::info!("Processing: {}", progress);
            }
        }
    }

    fn eta(&self) -> humantime::Duration {
        let seconds = self.bar.eta().as_secs();
        let sec_truncated_eta = std::time::Duration::from_secs(seconds);
        humantime::Duration::from(sec_truncated_eta)
    }
}

pub struct MultipleProgressReporter {
    multi_progress: MultiProgress,
}

impl MultipleProgressReporter {
    pub fn new() -> Self {
        Self {
            multi_progress: MultiProgress::new(),
        }
    }

    pub fn register(&self, reporter: ProgressReporter) -> ProgressReporter {
        let bar = self.multi_progress.add(reporter.bar);
        ProgressReporter { bar }
    }
}
