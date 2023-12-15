use super::{Gas, GasSizeOf};
use std::cmp::{max, min};

pub mod complexity {
    use super::*;

    // Args have additive linear complexity
    // Eg: O(N₁+N₂)
    pub struct Linear;
    // Args have multiplicative complexity
    // Eg: O(N₁*N₂)
    pub struct Mul;

    // Exponential complexity.
    // Eg: O(N₁^N₂)
    pub struct Exponential;

    // There is only one arg and it scales linearly with it's size
    pub struct Size;

    // Complexity is captured by the lesser complexity of the two args
    // Eg: O(min(N₁, N₂))
    pub struct Min;

    // Complexity is captured by the greater complexity of the two args
    // Eg: O(max(N₁, N₂))
    pub struct Max;

    impl GasCombinator for Linear {
        #[inline(always)]
        fn combine(lhs: Gas, rhs: Gas) -> Gas {
            lhs + rhs
        }
    }

    impl GasCombinator for Mul {
        #[inline(always)]
        fn combine(lhs: Gas, rhs: Gas) -> Gas {
            Gas(lhs.0.saturating_mul(rhs.0))
        }
    }

    impl GasCombinator for Min {
        #[inline(always)]
        fn combine(lhs: Gas, rhs: Gas) -> Gas {
            min(lhs, rhs)
        }
    }

    impl GasCombinator for Max {
        #[inline(always)]
        fn combine(lhs: Gas, rhs: Gas) -> Gas {
            max(lhs, rhs)
        }
    }

    impl<T> GasSizeOf for Combine<T, Size>
    where
        T: GasSizeOf,
    {
        fn gas_size_of(&self) -> Gas {
            self.0.gas_size_of()
        }
    }
}

pub struct Combine<Tuple, Combinator>(pub Tuple, pub Combinator);

pub trait GasCombinator {
    fn combine(lhs: Gas, rhs: Gas) -> Gas;
}

impl<T0, T1, C> GasSizeOf for Combine<(T0, T1), C>
where
    T0: GasSizeOf,
    T1: GasSizeOf,
    C: GasCombinator,
{
    fn gas_size_of(&self) -> Gas {
        let (a, b) = &self.0;
        C::combine(a.gas_size_of(), b.gas_size_of())
    }

    #[inline]
    fn const_gas_size_of() -> Option<Gas> {
        if let Some(t0) = T0::const_gas_size_of() {
            if let Some(t1) = T1::const_gas_size_of() {
                return Some(C::combine(t0, t1));
            }
        }
        None
    }
}

impl<T0, T1, T2, C> GasSizeOf for Combine<(T0, T1, T2), C>
where
    T0: GasSizeOf,
    T1: GasSizeOf,
    T2: GasSizeOf,
    C: GasCombinator,
{
    fn gas_size_of(&self) -> Gas {
        let (a, b, c) = &self.0;
        C::combine(
            C::combine(a.gas_size_of(), b.gas_size_of()),
            c.gas_size_of(),
        )
    }

    #[inline] // Const propagation to the rescue. I hope.
    fn const_gas_size_of() -> Option<Gas> {
        if let Some(t0) = T0::const_gas_size_of() {
            if let Some(t1) = T1::const_gas_size_of() {
                if let Some(t2) = T2::const_gas_size_of() {
                    return Some(C::combine(C::combine(t0, t1), t2));
                }
            }
        }
        None
    }
}

impl<T0> GasSizeOf for Combine<(T0, u8), complexity::Exponential>
where
    T0: GasSizeOf,
{
    fn gas_size_of(&self) -> Gas {
        let (a, b) = &self.0;
        Gas(a.gas_size_of().0.saturating_pow(*b as u32))
    }

    #[inline]
    fn const_gas_size_of() -> Option<Gas> {
        None
    }
}
