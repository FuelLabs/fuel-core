use fuel_core_metrics::compression_metrics::compression_metrics;

#[derive(Clone, Copy)]
pub(crate) struct CompressionMetricsManager;

impl CompressionMetricsManager {
    pub(crate) fn new() -> Self {
        CompressionMetricsManager
    }

    pub(crate) fn record_compressed_block_size(&self, compressed_size: usize) {
        compression_metrics()
            .compressed_block_size_bytes
            .set(u32::try_from(compressed_size).unwrap_or(u32::MAX));
    }

    pub(crate) fn record_compression_duration_ms(&self, duration_ms: f64) {
        compression_metrics()
            .compression_duration_ms
            .set(duration_ms);
    }

    pub(crate) fn record_compression_block_height(&self, height: u32) {
        compression_metrics().compression_block_height.set(height);
    }
}
