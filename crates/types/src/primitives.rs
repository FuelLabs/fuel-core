//! Common primitive types used across fuel-core.

/// A value that represents a value between 0 and 100. Higher values are clamped to 100
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ClampedPercentage {
    value: u8,
}

impl ClampedPercentage {
    /// Creates a new `ClampedPercentage` from a `u8` value.
    /// Values greater than 100 are clamped to 100.
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

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ClampedPercentage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize the struct, but ensure the value is clamped
        // We use a temporary struct to deserialize, then reconstruct with clamping
        #[derive(serde::Deserialize)]
        struct ClampedPercentageHelper {
            value: u8,
        }
        
        let helper = ClampedPercentageHelper::deserialize(deserializer)?;
        Ok(Self::new(helper.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_clamps_values_greater_than_100() {
        assert_eq!(ClampedPercentage::new(101).value, 100);
        assert_eq!(ClampedPercentage::new(150).value, 100);
        assert_eq!(ClampedPercentage::new(255).value, 100);
    }

    #[test]
    fn new_preserves_values_less_than_or_equal_to_100() {
        assert_eq!(ClampedPercentage::new(0).value, 0);
        assert_eq!(ClampedPercentage::new(50).value, 50);
        assert_eq!(ClampedPercentage::new(100).value, 100);
    }

    #[test]
    fn from_u8_works_correctly() {
        assert_eq!(ClampedPercentage::from(0).value, 0);
        assert_eq!(ClampedPercentage::from(50).value, 50);
        assert_eq!(ClampedPercentage::from(100).value, 100);
        assert_eq!(ClampedPercentage::from(101).value, 100);
        assert_eq!(ClampedPercentage::from(255).value, 100);
    }

    #[test]
    fn deref_works_correctly() {
        let percentage = ClampedPercentage::new(50);
        assert_eq!(*percentage, 50);
        assert_eq!(percentage.value, 50);
    }

    #[test]
    fn default_is_zero() {
        let default = ClampedPercentage::default();
        assert_eq!(default.value, 0);
        assert_eq!(*default, 0);
    }

    #[test]
    fn partial_eq_works() {
        assert_eq!(ClampedPercentage::new(50), ClampedPercentage::new(50));
        assert_ne!(ClampedPercentage::new(50), ClampedPercentage::new(51));
    }

    #[test]
    fn partial_ord_works() {
        assert!(ClampedPercentage::new(50) < ClampedPercentage::new(51));
        assert!(ClampedPercentage::new(100) > ClampedPercentage::new(50));
        assert!(ClampedPercentage::new(50) <= ClampedPercentage::new(50));
        assert!(ClampedPercentage::new(50) >= ClampedPercentage::new(50));
    }

    #[test]
    fn clone_and_copy_work() {
        let original = ClampedPercentage::new(75);
        let cloned = original;
        let copied = original.clone();
        assert_eq!(original, cloned);
        assert_eq!(original, copied);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_round_trip_with_postcard() {
        use postcard;

        let percentage = ClampedPercentage::new(50);
        let serialized = postcard::to_allocvec(&percentage).unwrap();
        let deserialized: ClampedPercentage = postcard::from_bytes(&serialized).unwrap();
        assert_eq!(percentage, deserialized);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_clamps_on_deserialize() {
        use postcard;

        // Test that deserializing a struct with value > 100 gets clamped
        // We create a helper struct with value > 100, serialize it,
        // then deserialize as ClampedPercentage to ensure clamping works
        #[derive(serde::Serialize)]
        struct TestHelper {
            value: u8,
        }
        
        let helper = TestHelper { value: 150 };
        let serialized = postcard::to_allocvec(&helper).unwrap();
        
        // Deserialize should clamp the value
        let deserialized: ClampedPercentage = postcard::from_bytes(&serialized).unwrap();
        assert_eq!(deserialized.value, 100, "Deserialized value > 100 should be clamped to 100");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_preserves_clamping() {
        use postcard;

        // Create a percentage with value > 100 (should be clamped)
        let percentage = ClampedPercentage::new(150);
        assert_eq!(percentage.value, 100);

        // Serialize and deserialize
        let serialized = postcard::to_allocvec(&percentage).unwrap();
        let deserialized: ClampedPercentage = postcard::from_bytes(&serialized).unwrap();
        assert_eq!(deserialized.value, 100);
    }
}
