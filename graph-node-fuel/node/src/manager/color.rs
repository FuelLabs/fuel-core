use std::sync::Mutex;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use graph::prelude::{isatty, lazy_static};

use super::CmdResult;

lazy_static! {
    static ref COLOR_MODE: Mutex<ColorChoice> = Mutex::new(ColorChoice::Auto);
}

/// A helper to generate colored terminal output
pub struct Terminal {
    out: StandardStream,
    spec: ColorSpec,
}

impl Terminal {
    pub fn set_color_preference(pref: &str) {
        let choice = match pref {
            "always" => ColorChoice::Always,
            "ansi" => ColorChoice::AlwaysAnsi,
            "auto" => {
                if isatty::stdout_isatty() {
                    ColorChoice::Auto
                } else {
                    ColorChoice::Never
                }
            }
            _ => ColorChoice::Never,
        };
        *COLOR_MODE.lock().unwrap() = choice;
    }

    fn color_preference() -> ColorChoice {
        *COLOR_MODE.lock().unwrap()
    }

    pub fn new() -> Self {
        Self {
            out: StandardStream::stdout(Self::color_preference()),
            spec: ColorSpec::new(),
        }
    }

    pub fn green(&mut self) -> CmdResult {
        self.spec.set_fg(Some(Color::Green));
        self.out.set_color(&self.spec).map_err(Into::into)
    }

    pub fn blue(&mut self) -> CmdResult {
        self.spec.set_fg(Some(Color::Blue));
        self.out.set_color(&self.spec).map_err(Into::into)
    }

    pub fn dim(&mut self) -> CmdResult {
        self.spec.set_dimmed(true);
        self.out.set_color(&self.spec).map_err(Into::into)
    }

    pub fn bold(&mut self) -> CmdResult {
        self.spec.set_bold(true);
        self.out.set_color(&self.spec).map_err(Into::into)
    }

    pub fn reset(&mut self) -> CmdResult {
        self.spec = ColorSpec::new();
        self.out.reset().map_err(Into::into)
    }
}

impl std::io::Write for Terminal {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.out.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}
