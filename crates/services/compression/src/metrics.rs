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

    pub(crate) fn record_compression_duration_ms(&self, duration_ms: u128) {
        // this cast is safe. we would never record a compression longer than 49 days
        // lets assume that we would be able to detect this and fix it before it happens :)
        compression_metrics()
            .compression_duration_ms
            .set(u32::try_from(duration_ms).unwrap_or(u32::MAX));
    }

    pub(crate) fn record_compression_block_height(&self, height: u32) {
        compression_metrics().compression_block_height.set(height);
    }
}
