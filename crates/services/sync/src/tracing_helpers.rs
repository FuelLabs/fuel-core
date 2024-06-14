use std::fmt::Display;

pub trait TraceErr {
    fn trace_err(self, msg: &str) -> Self;
}

impl<T, E> TraceErr for Result<T, E>
where
    E: Display,
{
    fn trace_err(self, msg: &str) -> Self {
        if let Err(e) = &self {
            tracing::error!("{} {}", msg, e);
        }
        self
    }
}
