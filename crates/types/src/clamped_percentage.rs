/// A value that represents a value between 0 and 100. Higher values are clamped to 100
#[derive(
    serde::Serialize, serde::Deserialize, Debug, Copy, Clone, PartialEq, PartialOrd,
)]
pub struct ClampedPercentage {
    value: u8,
}

impl ClampedPercentage {
     /// Creates a new `ClampedPercentage` by clamping the given value to the range [0, 100].
    pub fn new(maybe_value: u8) -> Self {
        Self {
            value: maybe_value.min(100),
        }
    }
}

impl From<u8> for ClampedPercentage {
    fn from(value: u8) -> Self {
        Self::new(value)
    }
}

impl core::ops::Deref for ClampedPercentage {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}