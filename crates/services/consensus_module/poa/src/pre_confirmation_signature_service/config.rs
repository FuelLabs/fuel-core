use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub key_rotation_interval: Duration,
    pub key_expiration_interval: Duration,
    pub echo_delegation_interval: Duration,
}
