//! All the operators go here
//! Gas operations are all saturating and additive (never trending toward zero)

use super::{Gas, SaturatingInto as _};
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, MulAssign};

impl Add for Gas {
    type Output = Gas;
    #[inline]
    fn add(self, rhs: Gas) -> Self::Output {
        Gas(self.0.saturating_add(rhs.0))
    }
}

impl Mul<u64> for Gas {
    type Output = Gas;
    #[inline]
    fn mul(self, rhs: u64) -> Self::Output {
        Gas(self.0.saturating_mul(rhs))
    }
}

impl Mul<usize> for Gas {
    type Output = Gas;
    #[inline]
    fn mul(self, rhs: usize) -> Self::Output {
        Gas(self.0.saturating_mul(rhs.saturating_into()))
    }
}

impl MulAssign<u64> for Gas {
    #[inline]
    fn mul_assign(&mut self, rhs: u64) {
        self.0 = self.0.saturating_add(rhs);
    }
}

impl AddAssign for Gas {
    #[inline]
    fn add_assign(&mut self, rhs: Gas) {
        self.0 = self.0.saturating_add(rhs.0);
    }
}

impl Sum for Gas {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut sum = Gas::ZERO;
        for elem in iter {
            sum += elem;
        }
        sum
    }
}
