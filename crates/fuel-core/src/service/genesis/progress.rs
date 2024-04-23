use std::{
    borrow::Cow,
    io::IsTerminal,
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
    target: ReportMethod,
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new(ReportMethod::Logs(tracing::info_span!("default")), None)
    }
}

/// Defines the method of reporting the progress of a [`ProgressReporter`].
#[derive(Clone)]
pub enum ReportMethod {
    /// Progress is to be reported on stderr visualized as a progress bar.
    VisualBar(String),
    /// Progress is to be reported via log messages.
    Logs(Span),
}

impl ProgressReporter {
    pub fn new(target: ReportMethod, max: Option<usize>) -> Self {
        let max = max.map(|max| u64::try_from(max).unwrap_or(u64::MAX));
        // Bars always hidden. Will be printed only if added to a `MultipleProgressReporter` that
        // prints to stderr. This removes flicker from double rendering (once when the progress bar
        // is constructed and again when added to the `MultipleProgressReporter`)
        let bar = ProgressBar::with_draw_target(max, ProgressDrawTarget::hidden());
        if let ReportMethod::VisualBar(message) = &target {
            bar.set_message(message.clone());

            bar.set_style(Self::style(max.is_some()));
        }

        ProgressReporter { bar, target }
    }

    fn style(length_known: bool) -> ProgressStyle {
        let template = if length_known {
            "[{elapsed_precise}] {bar:.64.on_black} {pos:>7}/{len:7} {msg} {eta}"
        } else {
            "[{elapsed_precise}] {pos:>7} {msg}"
        };

        ProgressStyle::with_template(template).expect("hard coded templates to be valid")
    }

    /// Sets the index of the last element handled.
    pub fn set_index(&self, index: usize) {
        // So that the last element shows up as, e.g., 100/100 and not 99/100.
        let display_index = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
        self.bar.set_position(display_index);
        if let ReportMethod::Logs(span) = &self.target {
            span.in_scope(|| {
                if let Some(len) = self.bar.length() {
                    let human_eta = HumanDuration(self.bar.eta());
                    tracing::info!("Processing: {display_index}/{len}. ({human_eta})");
                } else {
                    tracing::info!("Processing: {}", display_index);
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

    pub fn table_reporter(
        &self,
        num_groups: Option<usize>,
        desc: impl Into<Cow<'static, str>>,
    ) -> ProgressReporter {
        let target = if Self::should_display_bars() {
            ReportMethod::VisualBar(desc.into().into_owned())
        } else {
            let span = tracing::span!(
                parent: &self.span,
                Level::INFO,
                "task",
                migration = desc.into().as_ref()

            );
            ReportMethod::Logs(span)
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
