use fork::Fork;
use std::{
    io,
    io::{
        Read,
        Write,
    },
    sync::{
        Arc,
        Mutex,
        MutexGuard,
    },
};
use tracing::{
    dispatcher,
    info_span,
};
use tracing_core::Dispatch;
use tracing_subscriber::{
    EnvFilter,
    fmt::MakeWriter,
    layer::SubscriberExt,
    registry,
};

/// A fake writer that writes into a buffer (behind a mutex).
#[derive(Default, Debug, Clone)]
pub struct MockWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl MockWriter {
    /// Create a new `MockWriter` that writes into the specified buffer (behind a mutex).
    pub fn new(buf: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { buf }
    }

    /// Give access to the internal buffer (behind a `MutexGuard`).
    fn buf(&self) -> io::Result<MutexGuard<Vec<u8>>> {
        // Note: The `lock` will block. This would be a problem in production code,
        // but is fine in tests.
        self.buf
            .lock()
            .map_err(|_| io::Error::from(io::ErrorKind::Other))
    }
}

impl io::Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Lock target buffer
        let mut target = self.buf()?;
        target.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buf()?.flush()
    }
}

impl MakeWriter<'_> for MockWriter {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        MockWriter::new(self.buf.clone())
    }
}

/// Return a new subscriber that writes to the specified [`MockWriter`].
///
/// [`MockWriter`]: struct.MockWriter.html
pub fn get_subscriber(mock_writer: MockWriter) -> Dispatch {
    use tracing_subscriber::Layer;

    let fmt = tracing_subscriber::fmt::Layer::default()
        .with_writer(mock_writer)
        .with_ansi(false)
        .with_level(true)
        .with_line_number(true)
        .boxed();

    let registry = registry::Registry::default()
        .with(fmt)
        .with(EnvFilter::new("info"));
    registry.into()
}

/// Capture logs emitted during the execution of the provided closure.
pub fn capture_logs<T>(f: impl FnOnce() -> T) -> (T, Vec<String>)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    dispatcher::get_default(|_| {
        let span = info_span!("check");

        if span.id().is_some() {
            panic!("`capture_logs` requires default dispatcher to be none")
        }
    });

    let (mut pipe_reader, mut pipe_writer) =
        os_pipe::pipe().expect("Failed to create pipe");

    let result = fork::fork();

    match result {
        Ok(Fork::Child) => {
            let buffer: Arc<Mutex<Vec<u8>>> = Default::default();
            let writer = MockWriter::new(buffer.clone());
            let dispatch = get_subscriber(writer.clone());

            dispatcher::set_global_default(dispatch)
                .expect("Unable to set the global dispatcher");

            let result = f();

            let bytes =
                core::mem::take(&mut *buffer.lock().expect("failed to lock buffer"));

            let serialized = postcard::to_stdvec(&(result, bytes))
                .expect("failed to serialize result");

            let _ = pipe_writer
                .write(serialized.as_slice())
                .expect("failed to write to pipe");
            pipe_writer.flush().expect("failed to flush to pipe");

            std::process::exit(0);
        }
        Ok(Fork::Parent(pid)) => {
            drop(pipe_writer);

            fork::waitpid(pid).expect("failed to wait for child process");

            let mut serilized = vec![];
            pipe_reader
                .read_to_end(&mut serilized)
                .expect("failed to read from pipe");

            let (result, bytes): (T, Vec<u8>) =
                postcard::from_bytes(&serilized).expect("failed to deserialize result");

            let logs = String::from_utf8_lossy(&bytes);
            println!("{logs}");

            let logs: Vec<String> = logs.lines().map(|line| line.to_string()).collect();

            (result, logs)
        }
        Err(_) => {
            panic!("Fork failed");
        }
    }
}

/// Capture logs emitted during the execution of the provided future.
pub fn capture_logs_async<T>(f: impl Future<Output = T>) -> (T, Vec<String>)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    capture_logs(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime to extract logs the future")
            .block_on(f)
    })
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[test]
    fn local_dispatcher_gathers_logs() {
        const MESSAGE_1: &str = "Hello world!";
        const MESSAGE_2: &str = "Hello world 2!";

        let (_, logs) = capture_logs(|| {
            tracing::info!(MESSAGE_1);
            tracing::info!(MESSAGE_2);
        });

        assert!(logs[0].contains(MESSAGE_1));
        assert!(logs[1].contains(MESSAGE_2));
    }

    #[test]
    fn local_dispatcher_gathers_logs__and_keeps_empty_lines() {
        const MESSAGE: &str = "Hello world!";

        let (_, logs) = capture_logs(|| {
            tracing::info!("\n\n\n {} \n\n\n", MESSAGE);
        });
        assert!(logs[3].contains(MESSAGE));
    }

    #[test]
    fn local_dispatcher_gathers_logs__from_another_thread() {
        const MESSAGE: &str = "Hello world!";

        let (_, logs) = capture_logs(|| {
            std::thread::spawn(|| {
                tracing::info!("\n\n\n {} \n\n\n", MESSAGE);
            })
            .join()
            .unwrap();
        });
        assert!(logs[3].contains(MESSAGE));
    }

    #[test]
    fn local_dispatcher_gathers_logs_for_async_function() {
        const MESSAGE: &str = "Hello world!";

        let (_, logs) = capture_logs_async(async move {
            tracing::info!(MESSAGE);
        });

        assert!(logs[0].contains(MESSAGE));
    }
}
