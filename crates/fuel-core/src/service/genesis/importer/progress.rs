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

impl ProgressReporter {
    pub fn set_progress(&self, progress: u64) {
        self.bar.set_position(progress);
        if self.bar.is_hidden() {
            if let Some(len) = self.bar.length() {
                let eta = self.bar.eta();
                let human_eta = humantime::Duration::from(eta).to_string();

                tracing::info!("Processing: {}/{}. ({:?})", progress, len, human_eta);
            } else {
                tracing::info!("Processing: {}", progress);
            }
        }
    }
}

pub struct MultipleProgressReporter {
    multi_progress: MultiProgress,
    style: ProgressStyle,
}

impl MultipleProgressReporter {
    pub fn new() -> Self {
        let sty = ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg} {eta}",
        )
        .unwrap()
        .progress_chars("##-");
        Self {
            multi_progress: MultiProgress::new(),
            style: sty,
        }
    }

    pub fn reporter(&self, max: usize) -> ProgressReporter {
        if std::io::stderr().is_terminal() {
            self.visual_reporter(max)
        } else {
            self.log_reporter(max)
        }
    }

    pub fn visual_reporter(&self, max: usize) -> ProgressReporter {
        self.create_bar(max, ProgressDrawTarget::stderr())
    }

    pub fn log_reporter(&self, max: usize) -> ProgressReporter {
        let target = ProgressDrawTarget::hidden();
        self.create_bar(max, target)
    }

    fn create_bar(&self, max: usize, target: ProgressDrawTarget) -> ProgressReporter {
        let max = u64::try_from(max).unwrap_or(u64::MAX);
        let pb = ProgressBar::with_draw_target(Some(max), target);
        let pb = self.multi_progress.add(pb);
        pb.set_style(self.style.clone());
        ProgressReporter { bar: pb }
    }
}
