use super::Gas;
use std::convert::TryInto as _;

pub trait SaturatingFrom<T> {
    fn saturating_from(value: T) -> Self;
}

// It would be good to put this trait into a new or existing crate.
// Tried conv but the owner seems to be away
// https://github.com/DanielKeep/rust-conv/issues/15
pub trait SaturatingInto<T> {
    fn saturating_into(self) -> T;
}

impl<I, F> SaturatingInto<F> for I
where
    F: SaturatingFrom<I>,
{
    #[inline(always)]
    fn saturating_into(self) -> F {
        F::saturating_from(self)
    }
}

impl SaturatingFrom<usize> for Gas {
    #[inline]
    fn saturating_from(value: usize) -> Gas {
        Gas(value.try_into().unwrap_or(u64::MAX))
    }
}

impl SaturatingFrom<usize> for u64 {
    #[inline]
    fn saturating_from(value: usize) -> Self {
        value.try_into().unwrap_or(u64::MAX)
    }
}

impl SaturatingFrom<f64> for Gas {
    #[inline]
    fn saturating_from(value: f64) -> Self {
        Gas(value as u64)
    }
}

impl SaturatingFrom<u32> for Gas {
    #[inline]
    fn saturating_from(value: u32) -> Self {
        Gas(value as u64)
    }
}
