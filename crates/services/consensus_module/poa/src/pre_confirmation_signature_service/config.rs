use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub key_rotation_interval: Duration,
    pub key_expiration_interval: Duration,
    pub echo_delegation_interval: Duration,
}

#[cfg(any(test, feature = "test-helpers"))]
impl Default for Config {
    fn default() -> Self {
        Self {
            key_rotation_interval: Duration::from_secs(10),
            key_expiration_interval: Duration::from_secs(30),
            echo_delegation_interval: Duration::from_secs(5),
        }
    }
}
