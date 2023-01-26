# Fuel Core Trace
This crate provides a simple way to enable tracing for tests across the fuel repo.

## Usage
To enable tracing for a test, add the following to the lib.rs file of the crate you want to trace:
```rust
#[cfg(test)]
fuel_core_trace::enable_tracing!();
```
If you also want tracing in your integration tests, you need to add the above to each integration tests file.

Tracing is still disabled by default so to enable it set the following environment variable:
```bash
export FUEL_TRACE=1
```
Or if you just want it for a single test run:
```bash
FUEL_TRACE=1 cargo test
```

Now you will have error level tracing like:
```
2023-01-25T02:27:14.362856Z ERROR works: tracing: I'm visible if FUEL_TRACE=1 is set
```

You can use the `RUST_LOG` environment variable to control the level of tracing you want to see. For example:
```bash
FUEL_TRACE=1 RUST_LOG=trace cargo test
```
### Additional Subscribers
You can set a few different types of subscribers:

Compact output:
```bash
FUEL_TRACE=compact cargo test
```
Pretty output:
```bash
FUEL_TRACE=pretty cargo test
```
Log to file:
```bash
FUEL_TRACE=log-file cargo test
```
Log to file and output to console:
```bash
FUEL_TRACE=log-show cargo test
```
You can also set the log file path:
```bash
FUEL_TRACE_PATH=/some/path FUEL_TRACE=log-file cargo test
```
If you don't set the path, it will default to `CARGO_MANIFEST_DIR/logs/logfile`.
