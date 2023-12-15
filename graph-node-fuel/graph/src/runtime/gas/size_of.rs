//! Various implementations of GasSizeOf;

use crate::{
    components::store::LoadRelatedRequest,
    data::store::{scalar::Bytes, Value},
    schema::{EntityKey, EntityType},
};

use super::{Gas, GasSizeOf, SaturatingInto as _};

impl GasSizeOf for Value {
    fn gas_size_of(&self) -> Gas {
        let inner = match self {
            Value::BigDecimal(big_decimal) => big_decimal.gas_size_of(),
            Value::String(string) => string.gas_size_of(),
            Value::Null => Gas(1),
            Value::List(list) => list.gas_size_of(),
            Value::Int(int) => int.gas_size_of(),
            Value::Int8(int) => int.gas_size_of(),
            Value::Bytes(bytes) => bytes.gas_size_of(),
            Value::Bool(bool) => bool.gas_size_of(),
            Value::BigInt(big_int) => big_int.gas_size_of(),
        };
        Gas(4) + inner
    }
}

impl GasSizeOf for Bytes {
    fn gas_size_of(&self) -> Gas {
        (&self[..]).gas_size_of()
    }
}

impl GasSizeOf for bool {
    #[inline(always)]
    fn const_gas_size_of() -> Option<Gas> {
        Some(Gas(1))
    }
}

impl<T> GasSizeOf for Option<T>
where
    T: GasSizeOf,
{
    fn gas_size_of(&self) -> Gas {
        if let Some(v) = self {
            Gas(1) + v.gas_size_of()
        } else {
            Gas(1)
        }
    }
}

impl GasSizeOf for str {
    fn gas_size_of(&self) -> Gas {
        self.len().saturating_into()
    }
}

impl GasSizeOf for String {
    fn gas_size_of(&self) -> Gas {
        self.as_str().gas_size_of()
    }
}

impl<K, V, S> GasSizeOf for std::collections::HashMap<K, V, S>
where
    K: GasSizeOf,
    V: GasSizeOf,
{
    fn gas_size_of(&self) -> Gas {
        let members = match (K::const_gas_size_of(), V::const_gas_size_of()) {
            (Some(k_gas), None) => {
                self.values().map(|v| v.gas_size_of()).sum::<Gas>() + k_gas * self.len()
            }
            (None, Some(v_gas)) => {
                self.keys().map(|k| k.gas_size_of()).sum::<Gas>() + v_gas * self.len()
            }
            (Some(k_gas), Some(v_gas)) => (k_gas + v_gas) * self.len(),
            (None, None) => self
                .iter()
                .map(|(k, v)| k.gas_size_of() + v.gas_size_of())
                .sum(),
        };
        members + Gas(32) + (Gas(8) * self.len())
    }
}

impl<T> GasSizeOf for &[T]
where
    T: GasSizeOf,
{
    fn gas_size_of(&self) -> Gas {
        if let Some(gas) = T::const_gas_size_of() {
            gas * self.len()
        } else {
            self.iter().map(|e| e.gas_size_of()).sum()
        }
    }
}

impl<T> GasSizeOf for Vec<T>
where
    T: GasSizeOf,
{
    fn gas_size_of(&self) -> Gas {
        let members = (&self[..]).gas_size_of();
        // Overhead for Vec so that Vec<Vec<T>> is more expensive than Vec<T>
        members + Gas(16) + self.len().saturating_into()
    }
}

impl<T: ?Sized> GasSizeOf for &T
where
    T: GasSizeOf,
{
    #[inline(always)]
    fn gas_size_of(&self) -> Gas {
        <T as GasSizeOf>::gas_size_of(*self)
    }

    #[inline(always)]
    fn const_gas_size_of() -> Option<Gas> {
        T::const_gas_size_of()
    }
}

macro_rules! int_gas {
    ($($name: ident),*) => {
        $(
            impl GasSizeOf for $name {
                #[inline(always)]
                fn const_gas_size_of() -> Option<Gas> {
                    Some(std::mem::size_of::<$name>().saturating_into())
                }
            }
        )*
    }
}

int_gas!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128);

impl GasSizeOf for usize {
    fn const_gas_size_of() -> Option<Gas> {
        // Must be the same regardless of platform.
        u64::const_gas_size_of()
    }
}

impl GasSizeOf for EntityKey {
    fn gas_size_of(&self) -> Gas {
        self.entity_type.gas_size_of() + self.entity_id.gas_size_of()
    }
}

impl GasSizeOf for LoadRelatedRequest {
    fn gas_size_of(&self) -> Gas {
        self.entity_type.gas_size_of()
            + self.entity_id.gas_size_of()
            + self.entity_field.gas_size_of()
    }
}

impl GasSizeOf for EntityType {
    fn gas_size_of(&self) -> Gas {
        self.as_str().gas_size_of()
    }
}
