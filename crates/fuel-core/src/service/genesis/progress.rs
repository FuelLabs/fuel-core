use std::{
    borrow::Cow,
    io::IsTerminal,
};

use fuel_core_storage::{
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
};
use indicatif::{
    HumanDuration,
    MultiProgress,
    ProgressBar,
    ProgressDrawTarget,
    ProgressStyle,
};
use tracing::{
    Level,
    Span,
};

#[derive(Clone)]
pub struct ProgressReporter {
    bar: ProgressBar,
    target: Target,
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new(Target::Logs(tracing::info_span!("default")), None)
    }
}

#[derive(Clone)]
pub enum Target {
    Cli(String),
    Logs(Span),
}

impl ProgressReporter {
    pub fn new(target: Target, max: Option<usize>) -> Self {
        let max = max.map(|max| u64::try_from(max).unwrap_or(u64::MAX));
        // Bars always hidden. Will be printed only if added to a `MultipleProgressReporter` that
        // prints to stderr. This removes flicker from double rendering (once when the progress bar
        // is constructed and again when added to the `MultipleProgressReporter`)
        let bar = ProgressBar::with_draw_target(max, ProgressDrawTarget::hidden());
        if let Target::Cli(message) = &target {
            bar.set_message(message.clone());

            bar.set_style(Self::style(max.is_some()));
        }

        ProgressReporter { bar, target }
    }

    fn style(lenght_known: bool) -> ProgressStyle {
        let template = if lenght_known {
            "[{elapsed_precise}] {bar:.64.on_black} {pos:>7}/{len:7} {msg} {eta}"
        } else {
            "[{elapsed_precise}] {pos:>7} {msg}"
        };

        ProgressStyle::with_template(template).expect("hard coded templates to be valid")
    }

    pub fn set_progress(&self, group_index: usize) {
        let group_num = u64::try_from(group_index)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
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
    span: Span,
}

impl MultipleProgressReporter {
    fn should_display_bars() -> bool {
        std::io::stderr().is_terminal() && !cfg!(test)
    }

    pub fn new(span: Span) -> MultipleProgressReporter {
        if Self::should_display_bars() {
            Self::new_target(ProgressDrawTarget::stderr(), span)
        } else {
            Self::new_target(ProgressDrawTarget::hidden(), span)
        }
    }

    pub fn table_reporter<T>(&self, num_groups: Option<usize>) -> ProgressReporter
    where
        T: TableWithBlueprint,
    {
        let desc = T::column().name();
        let target = if Self::should_display_bars() {
            Target::Cli(desc.to_owned())
        } else {
            let span = tracing::span!(
                parent: &self.span,
                Level::INFO,
                "task",
                migration = desc

            );
            Target::Logs(span)
        };

        self.register(ProgressReporter::new(target, num_groups))
    }

    fn new_target(target: ProgressDrawTarget, span: Span) -> Self {
        Self {
            multi_progress: MultiProgress::with_draw_target(target),
            span,
        }
    }

    fn register(&self, reporter: ProgressReporter) -> ProgressReporter {
        let bar = self.multi_progress.add(reporter.bar);
        ProgressReporter {
            bar,
            target: reporter.target,
        }
    }
}
