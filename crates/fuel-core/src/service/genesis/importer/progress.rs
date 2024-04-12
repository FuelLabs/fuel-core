use indicatif::{
    HumanDuration,
    MultiProgress,
    ProgressBar,
    ProgressDrawTarget,
    ProgressStyle,
};
use tracing::Span;

#[derive(Clone)]
pub struct ProgressReporter {
    bar: ProgressBar,
    target: Target,
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new(Target::Logs(tracing::info_span!("default")), usize::MAX)
    }
}

#[derive(Clone)]
pub enum Target {
    Cli(&'static str),
    Logs(Span),
}

impl ProgressReporter {
    pub fn new(target: Target, max: usize) -> Self {
        let max = u64::try_from(max).unwrap_or(u64::MAX);
        // Bars always hidden. Will be printed only if added to a `MultipleProgressReporter` that
        // prints to stderr. This removes flicker from double rendering (once when the progress bar
        // is constructed and again when added to the `MultipleProgressReporter`)
        let bar = ProgressBar::with_draw_target(Some(max), ProgressDrawTarget::hidden());
        if let Target::Cli(message) = target {
            bar.set_message(message);
            let style = ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:.64.on_black} {pos:>7}/{len:7} {msg} {eta}",
            )
            .unwrap();

            bar.set_style(style);
        }

        ProgressReporter { bar, target }
    }

    pub fn set_progress(&self, group_index: u64) {
        let group_num = group_index.saturating_add(1);
        self.bar.set_position(group_num);
        if let Target::Logs(span) = &self.target {
            span.in_scope(|| {
                if let Some(len) = self.bar.length() {
                    let human_eta = HumanDuration(self.bar.eta());
                    tracing::info!("Processing: {group_num}/{len}. ({human_eta})");
                } else {
                    tracing::info!("Processing: {}", group_num);
                }
            })
        }
    }
}

pub struct MultipleProgressReporter {
    multi_progress: MultiProgress,
}

impl MultipleProgressReporter {
    pub fn new_sterr() -> Self {
        Self::new_target(ProgressDrawTarget::stderr())
    }

    pub fn new_hidden() -> Self {
        Self::new_target(ProgressDrawTarget::hidden())
    }

    fn new_target(target: ProgressDrawTarget) -> Self {
        Self {
            multi_progress: MultiProgress::with_draw_target(target),
        }
    }

    pub fn register(&self, reporter: ProgressReporter) -> ProgressReporter {
        let bar = self.multi_progress.add(reporter.bar);
        ProgressReporter {
            bar,
            target: reporter.target,
        }
    }
}
