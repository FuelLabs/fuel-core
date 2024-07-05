use crate::global_registry;
use prometheus_client::metrics::counter::Counter;
use std::collections::HashMap;

#[derive(Debug)]
pub struct DatabaseMetrics {
    // For descriptions of each Counter, see the `new` function where each Counter/Histogram is initialized
    pub write_meter: Counter,
    pub read_meter: Counter,
    pub bytes_written: Counter,
    pub bytes_read: Counter,
    pub database_commit_time: Counter,
    pub columns_read_statistic: HashMap<u32, Counter>,
    pub columns_write_statistic: HashMap<u32, Counter>,
}

impl DatabaseMetrics {
    pub fn new(name: &str, columns: &[(u32, String)]) -> Self {
        let mut registry = global_registry().registry.lock();

        let columns_read_statistic = columns
            .iter()
            .map(|(column_id, column_name)| {
                let counter: Counter = Counter::default();
                registry.register(
                    format!("{}_Column_{}_Reads", name, column_name),
                    format!(
                        "Number of {} read operations on column {}",
                        name, column_name
                    ),
                    counter.clone(),
                );
                (*column_id, counter)
            })
            .collect();

        let columns_write_statistic = columns
            .iter()
            .map(|(column_id, column_name)| {
                let counter: Counter = Counter::default();
                registry.register(
                    format!("{}_Column_{}_Writes", name, column_name),
                    format!(
                        "Number of {} write operations on column {}",
                        name, column_name
                    ),
                    counter.clone(),
                );
                (*column_id, counter)
            })
            .collect();

        let write_meter: Counter = Counter::default();
        let read_meter: Counter = Counter::default();
        let bytes_written = Counter::default();
        let bytes_read = Counter::default();
        let database_commit_time: Counter = Counter::default();

        registry.register(
            format!("{}_Database_Writes", name),
            format!("Number of {} database write operations", name),
            write_meter.clone(),
        );
        registry.register(
            format!("{}_Database_Reads", name),
            format!("Number of {} database read operations", name),
            read_meter.clone(),
        );
        registry.register(
            format!("{}_Bytes_Read", name),
            format!("The total amount of read bytes from {}", name),
            bytes_read.clone(),
        );
        registry.register(
            format!("{}_Bytes_Written", name),
            format!("The total amount of written bytes into {}", name),
            bytes_written.clone(),
        );
        registry.register(
            format!("{}_Database_Commit_Time", name),
            format!(
                "The total commit time of the {} database including all sub-databases",
                name
            ),
            database_commit_time.clone(),
        );

        DatabaseMetrics {
            write_meter,
            read_meter,
            bytes_read,
            bytes_written,
            database_commit_time,
            columns_read_statistic,
            columns_write_statistic,
        }
    }
}
