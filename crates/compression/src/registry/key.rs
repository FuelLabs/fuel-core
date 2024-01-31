use core::fmt;
use std::marker::PhantomData;

use serde::{
    Deserialize,
    Serialize,
};

use super::Table;

/// Untyped key pointing to a registry table entry.
/// The last key (all bits set) is reserved for the default value and cannot be written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RawKey([u8; Self::SIZE]);
impl RawKey {
    pub const SIZE: usize = 3;
    pub const ZERO: Self = Self([0; Self::SIZE]);
    pub const MAX_WRITABLE: Self = Self([u8::MAX; Self::SIZE]);
    pub const DEFAULT_VALUE: Self = Self([u8::MAX; Self::SIZE]);

    pub fn as_u32(self) -> u32 {
        u32::from_be_bytes([0, self.0[0], self.0[1], self.0[2]])
    }

    /// Wraps around just below max/default value.
    pub fn add_u32(self, rhs: u32) -> Self {
        let lhs = self.as_u32();
        let v = lhs.wrapping_add(rhs);
        let b = v.to_be_bytes();
        let raw = Self([b[1], b[2], b[3]]);
        if raw == Self::DEFAULT_VALUE {
            Self::ZERO
        } else {
            raw
        }
    }

    /// Wraps around just below max/default value.
    pub fn next(self) -> Self {
        self.add_u32(1)
    }

    /// Is `self` between `start` and `end`? i.e. in the half-open logical range `start`..`end`,
    /// so that wrap-around cases are handled correctly.
    ///
    /// Panics if max/default value is used.
    pub fn is_between(self, start: Self, end: Self) -> bool {
        assert!(
            self != Self::DEFAULT_VALUE,
            "Cannot use max/default value in is_between"
        );
        assert!(
            start != Self::DEFAULT_VALUE,
            "Cannot use max/default value in is_between"
        );
        assert!(
            end != Self::DEFAULT_VALUE,
            "Cannot use max/default value in is_between"
        );

        let low = start.as_u32();
        let high = end.as_u32();
        let v = self.as_u32();

        if high >= low {
            low <= v && v < high
        } else {
            v < high || v >= low
        }
    }
}
impl TryFrom<u32> for RawKey {
    type Error = &'static str;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let v = value.to_be_bytes();
        if v[0] != 0 {
            return Err("RawKey must be less than 2^24");
        }

        let mut bytes = [0u8; 3];
        bytes.copy_from_slice(&v[1..]);
        Ok(Self(bytes))
    }
}

/// Typed key to a registry table entry.
/// The last key (all bits set) is reserved for the default value and cannot be written to.
#[derive(Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Key<T: Table>(RawKey, PhantomData<T>);
impl<T: Table> Clone for Key<T> {
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}
impl<T: Table> Copy for Key<T> {}

impl<T: Table> PartialEq<Key<T>> for Key<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Table> Key<T> {
    /// This is the first writable key.
    pub const ZERO: Self = Self(RawKey::ZERO, PhantomData);

    /// This key is reserved for the default value and cannot be written to.
    pub const DEFAULT_VALUE: Self = Self(RawKey::DEFAULT_VALUE, PhantomData);

    pub fn raw(&self) -> RawKey {
        self.0
    }

    pub fn from_raw(raw: RawKey) -> Self {
        Self(raw, PhantomData)
    }

    /// Wraps around at limit, i.e. one below the max/default value
    pub fn add_u32(self, rhs: u32) -> Self {
        Self(self.0.add_u32(rhs), PhantomData)
    }

    /// Wraps around at limit, i.e. one below the max/default value
    pub fn next(self) -> Self {
        Self(self.0.next(), PhantomData)
    }

    /// Is `self` between `start` and `end`? i.e. in the half-open logical range `start`..`end`,
    /// so that wrap-around cases are handled correctly.
    pub fn is_between(self, start: Self, end: Self) -> bool {
        self.0.is_between(start.0, end.0)
    }
    /// Increments the key by one, and returns the previous value.
    /// Skips the max/default value.
    pub fn take_next(&mut self) -> Self {
        let result = *self;
        self.0 = self.0.next();
        result
    }
}

impl<T: Table> TryFrom<u32> for Key<T> {
    type Error = &'static str;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(Self(RawKey::try_from(value)?, PhantomData))
    }
}

impl<T: Table> fmt::Debug for Key<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::DEFAULT_VALUE {
            write!(f, "Key<{}>::DEFAULT_VALUE", T::NAME)
        } else {
            write!(f, "Key<{}>({})", T::NAME, self.0.as_u32())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RawKey;

    #[test]
    fn key_next() {
        assert_eq!(RawKey::ZERO.next(), RawKey([0, 0, 1]));
        assert_eq!(RawKey::ZERO.next().next(), RawKey([0, 0, 2]));
        assert_eq!(RawKey([0, 0, 255]).next(), RawKey([0, 1, 0]));
        assert_eq!(RawKey([0, 1, 255]).next(), RawKey([0, 2, 0]));
        assert_eq!(RawKey([0, 255, 255]).next(), RawKey([1, 0, 0]));
        assert_eq!(RawKey::MAX_WRITABLE.next(), RawKey::ZERO);
    }
}
