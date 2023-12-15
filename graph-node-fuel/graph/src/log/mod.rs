#[macro_export]
macro_rules! impl_slog_value {
    ($T:ty) => {
        impl_slog_value!($T, "{}");
    };
    ($T:ty, $fmt:expr) => {
        impl $crate::slog::Value for $T {
            fn serialize(
                &self,
                record: &$crate::slog::Record,
                key: $crate::slog::Key,
                serializer: &mut dyn $crate::slog::Serializer,
            ) -> $crate::slog::Result {
                format!($fmt, self).serialize(record, key, serializer)
            }
        }
    };
}

use isatty;
use slog::*;
use slog_async;
use slog_envlogger;
use slog_term::*;
use std::{fmt, io, result};

use crate::prelude::ENV_VARS;

pub mod codes;
pub mod elastic;
pub mod factory;
pub mod split;

pub fn logger(show_debug: bool) -> Logger {
    logger_with_levels(show_debug, ENV_VARS.log_levels.as_deref())
}

pub fn logger_with_levels(show_debug: bool, levels: Option<&str>) -> Logger {
    let use_color = isatty::stdout_isatty();
    let decorator = slog_term::TermDecorator::new().build();
    let drain = CustomFormat::new(decorator, use_color).fuse();
    let drain = slog_envlogger::LogBuilder::new(drain)
        .filter(
            None,
            if show_debug {
                FilterLevel::Debug
            } else {
                FilterLevel::Info
            },
        )
        .parse(levels.unwrap_or(""))
        .build();
    let drain = slog_async::Async::new(drain)
        .chan_size(20000)
        .build()
        .fuse();
    Logger::root(drain, o!())
}

pub fn discard() -> Logger {
    Logger::root(slog::Discard, o!())
}

pub struct CustomFormat<D>
where
    D: Decorator,
{
    decorator: D,
    use_color: bool,
}

impl<D> Drain for CustomFormat<D>
where
    D: Decorator,
{
    type Ok = ();
    type Err = io::Error;

    fn log(&self, record: &Record, values: &OwnedKVList) -> result::Result<Self::Ok, Self::Err> {
        self.format_custom(record, values)
    }
}

impl<D> CustomFormat<D>
where
    D: Decorator,
{
    pub fn new(decorator: D, use_color: bool) -> Self {
        CustomFormat {
            decorator,
            use_color,
        }
    }

    fn format_custom(&self, record: &Record, values: &OwnedKVList) -> io::Result<()> {
        self.decorator.with_record(record, values, |mut decorator| {
            decorator.start_timestamp()?;
            formatted_timestamp_local(&mut decorator)?;
            decorator.start_whitespace()?;
            write!(decorator, " ")?;

            decorator.start_level()?;
            write!(decorator, "{}", record.level())?;

            decorator.start_whitespace()?;
            write!(decorator, " ")?;

            decorator.start_msg()?;
            // Escape control characters in the message, including newlines.
            let msg = escape_control_chars(record.msg().to_string());
            write!(decorator, "{}", msg)?;

            // Collect key values from the record
            let mut serializer = KeyValueSerializer::new();
            record.kv().serialize(record, &mut serializer)?;
            let body_kvs = serializer.finish();

            // Collect subgraph ID, components and extra key values from the record
            let mut serializer = HeaderSerializer::new();
            values.serialize(record, &mut serializer)?;
            let (subgraph_id, components, header_kvs) = serializer.finish();

            // Regular key values first
            for (k, v) in body_kvs.iter().chain(header_kvs.iter()) {
                decorator.start_comma()?;
                write!(decorator, ", ")?;

                decorator.start_key()?;
                write!(decorator, "{}", k)?;

                decorator.start_separator()?;
                write!(decorator, ": ")?;

                decorator.start_value()?;
                write!(decorator, "{}", v)?;
            }

            // Then log the subgraph ID (if present)
            if let Some(subgraph_id) = subgraph_id.as_ref() {
                decorator.start_comma()?;
                write!(decorator, ", ")?;
                decorator.start_key()?;
                write!(decorator, "subgraph_id")?;
                decorator.start_separator()?;
                write!(decorator, ": ")?;
                decorator.start_value()?;
                if self.use_color {
                    write!(decorator, "\u{001b}[35m{}\u{001b}[0m", subgraph_id)?;
                } else {
                    write!(decorator, "{}", subgraph_id)?;
                }
            }

            // Then log the component hierarchy
            if !components.is_empty() {
                decorator.start_comma()?;
                write!(decorator, ", ")?;
                decorator.start_key()?;
                write!(decorator, "component")?;
                decorator.start_separator()?;
                write!(decorator, ": ")?;
                decorator.start_value()?;
                if self.use_color {
                    write!(
                        decorator,
                        "\u{001b}[36m{}\u{001b}[0m",
                        components.join(" > ")
                    )?;
                } else {
                    write!(decorator, "{}", components.join(" > "))?;
                }
            }

            writeln!(decorator)?;
            decorator.flush()?;

            Ok(())
        })
    }
}

struct HeaderSerializer {
    subgraph_id: Option<String>,
    components: Vec<String>,
    kvs: Vec<(String, String)>,
}

impl HeaderSerializer {
    pub fn new() -> Self {
        Self {
            subgraph_id: None,
            components: vec![],
            kvs: vec![],
        }
    }

    pub fn finish(mut self) -> (Option<String>, Vec<String>, Vec<(String, String)>) {
        // Reverse components so the parent components come first
        self.components.reverse();

        (self.subgraph_id, self.components, self.kvs)
    }
}

macro_rules! s(
    ($s:expr, $k:expr, $v:expr) => {
        Ok(match $k {
            "component" => $s.components.push(format!("{}", $v)),
            "subgraph_id" => $s.subgraph_id = Some(format!("{}", $v)),
            _ => $s.kvs.push(($k.into(), format!("{}", $v))),
        })
    };
);

impl ser::Serializer for HeaderSerializer {
    fn emit_none(&mut self, key: Key) -> slog::Result {
        s!(self, key, "None")
    }

    fn emit_unit(&mut self, key: Key) -> slog::Result {
        s!(self, key, "()")
    }

    fn emit_bool(&mut self, key: Key, val: bool) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_char(&mut self, key: Key, val: char) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_usize(&mut self, key: Key, val: usize) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_isize(&mut self, key: Key, val: isize) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_u8(&mut self, key: Key, val: u8) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_i8(&mut self, key: Key, val: i8) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_u16(&mut self, key: Key, val: u16) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_i16(&mut self, key: Key, val: i16) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_u32(&mut self, key: Key, val: u32) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_i32(&mut self, key: Key, val: i32) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_f32(&mut self, key: Key, val: f32) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_u64(&mut self, key: Key, val: u64) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_i64(&mut self, key: Key, val: i64) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_f64(&mut self, key: Key, val: f64) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_str(&mut self, key: Key, val: &str) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> slog::Result {
        s!(self, key, val)
    }
}

struct KeyValueSerializer {
    kvs: Vec<(String, String)>,
}

impl KeyValueSerializer {
    pub fn new() -> Self {
        Self { kvs: vec![] }
    }

    pub fn finish(self) -> Vec<(String, String)> {
        self.kvs
    }
}

macro_rules! s(
    ($s:expr, $k:expr, $v:expr) => {
        Ok($s.kvs.push(($k.into(), format!("{}", $v))))
    };
);

impl ser::Serializer for KeyValueSerializer {
    fn emit_none(&mut self, key: Key) -> slog::Result {
        s!(self, key, "None")
    }

    fn emit_unit(&mut self, key: Key) -> slog::Result {
        s!(self, key, "()")
    }

    fn emit_bool(&mut self, key: Key, val: bool) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_char(&mut self, key: Key, val: char) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_usize(&mut self, key: Key, val: usize) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_isize(&mut self, key: Key, val: isize) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_u8(&mut self, key: Key, val: u8) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_i8(&mut self, key: Key, val: i8) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_u16(&mut self, key: Key, val: u16) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_i16(&mut self, key: Key, val: i16) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_u32(&mut self, key: Key, val: u32) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_i32(&mut self, key: Key, val: i32) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_f32(&mut self, key: Key, val: f32) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_u64(&mut self, key: Key, val: u64) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_i64(&mut self, key: Key, val: i64) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_f64(&mut self, key: Key, val: f64) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_str(&mut self, key: Key, val: &str) -> slog::Result {
        s!(self, key, val)
    }

    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> slog::Result {
        s!(self, key, val)
    }
}

fn formatted_timestamp_local(io: &mut impl io::Write) -> io::Result<()> {
    write!(
        io,
        "{}",
        chrono::Local::now().format(ENV_VARS.log_time_format.as_str())
    )
}

pub fn escape_control_chars(input: String) -> String {
    let should_escape = |c: char| c.is_control() && c != '\t';

    if !input.chars().any(should_escape) {
        return input;
    }

    let mut escaped = String::new();
    for c in input.chars() {
        match c {
            '\n' => escaped.push_str("\\n"),
            c if should_escape(c) => {
                let code = c as u32;
                escaped.push_str(&format!("\\u{{{:04x}}}", code));
            }
            _ => escaped.push(c),
        }
    }
    escaped
}

#[test]
fn test_escape_control_chars() {
    let test_cases = vec![
        (
            "This is a test\nwith some\tcontrol characters\x1B[1;32m and others.",
            "This is a test\\nwith some\tcontrol characters\\u{001b}[1;32m and others.",
        ),
        (
            "This string has no control characters.",
            "This string has no control characters.",
        ),
        (
            "This string has a tab\tbut no other control characters.",
            "This string has a tab\tbut no other control characters.",
        ),
    ];

    for (input, expected) in test_cases {
        let escaped = escape_control_chars(input.to_string());
        assert_eq!(escaped, expected);
    }
}
