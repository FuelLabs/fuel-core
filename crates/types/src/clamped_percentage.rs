/// A value that represents a value between 0 and 100. Higher values are clamped to 100
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
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

#[cfg(feature = "std")]
use std::str::FromStr;

#[cfg(feature = "std")]
impl FromStr for ClampedPercentage {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.parse::<u8>()?;
        Ok(ClampedPercentage::new(value))
    }
}

#[cfg(feature = "std")]
impl std::fmt::Display for ClampedPercentage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamping() {
        assert_eq!(*ClampedPercentage::new(50), 50);
        assert_eq!(*ClampedPercentage::new(150), 100);
        assert_eq!(*ClampedPercentage::new(0), 0);
    }
}