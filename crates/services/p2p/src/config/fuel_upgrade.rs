/// Sha256 hash of ChainConfig
#[derive(Debug, Clone, Copy, Default)]
pub struct Checksum([u8; 32]);

impl AsRef<[u8]> for Checksum {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for Checksum {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}
