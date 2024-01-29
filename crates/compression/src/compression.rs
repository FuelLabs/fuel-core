use std::marker::PhantomData;

use crate::{
    registry::{
        access::{
            self,
            *,
        },
        add_keys,
        block_section::WriteTo,
        db,
        next_keys,
        ChangesPerTable,
        CountPerTable,
        KeyPerTable,
        Table,
    },
    Key,
};

#[must_use]
pub struct CompactionContext<'a, R> {
    /// The registry
    reg: &'a mut R,
    /// These are the keys where writing started
    start_keys: KeyPerTable,
    /// The next keys to use for each table
    next_keys: KeyPerTable,
    /// Keys in range next_keys..safe_keys_start
    /// could be overwritten by the compaction,
    /// and cannot be used for new values.
    safe_keys_start: KeyPerTable,
    changes: ChangesPerTable,
}
impl<'a, R> CompactionContext<'a, R>
where
    R: db::RegistrySelectNextKey
        + db::RegistryRead
        + db::RegistryIndex
        + db::RegistryWrite,
{
    pub fn run<C: Compactable>(reg: &'a mut R, target: C) -> C::Compact {
        let start_keys = next_keys(reg);
        let next_keys = start_keys;
        let key_limits = target.count();
        let safe_keys_start = add_keys(next_keys, key_limits);

        let mut ctx = Self {
            reg,
            start_keys,
            next_keys,
            safe_keys_start,
            changes: Default::default(),
        };

        let compacted = target.compact(&mut ctx);
        ctx.changes.apply_to_registry(ctx.reg);
        compacted
    }
}

impl<'a, R> CompactionContext<'a, R>
where
    R: db::RegistryRead + db::RegistryIndex,
{
    /// Convert a value to a key
    /// If necessary, store the value in the changeset and allocate a new key.
    pub fn to_key<T: Table>(&mut self, value: T::Type) -> Key<T>
    where
        KeyPerTable: access::AccessCopy<T, Key<T>>,
        KeyPerTable: access::AccessMut<T, Key<T>>,
        ChangesPerTable: access::AccessMut<T, WriteTo<T>>,
    {
        // Check if the registry contains this value already
        if let Some(key) = self.reg.index_lookup::<T>(&value) {
            let start: Key<T> = self.start_keys.value();
            let end: Key<T> = self.safe_keys_start.value();
            // Check if the value is in the possibly-overwritable range
            if !key.is_between(start, end) {
                return key;
            }
        }
        // Allocate a new key for this
        let key = <KeyPerTable as AccessMut<T, Key<T>>>::get_mut(&mut self.next_keys)
            .take_next();
        <ChangesPerTable as access::AccessMut<T, WriteTo<T>>>::get_mut(&mut self.changes)
            .values
            .push(value);
        key
    }
}

/// Convert data to reference-based format
pub trait Compactable {
    type Compact;

    /// Count max number of each key type, for upper limit of overwritten keys
    fn count(&self) -> CountPerTable;

    fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
    where
        R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex;

    fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
    where
        R: db::RegistryRead;
}

macro_rules! identity_compaction {
    ($t:ty) => {
        impl Compactable for $t {
            type Compact = Self;

            fn count(&self) -> CountPerTable {
                CountPerTable::default()
            }

            fn compact<R>(&self, _ctx: &mut CompactionContext<R>) -> Self::Compact
            where
                R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
            {
                *self
            }

            fn decompact<R>(compact: Self::Compact, _reg: &R) -> Self
            where
                R: db::RegistryRead,
            {
                compact
            }
        }
    };
}

identity_compaction!(u8);
identity_compaction!(u16);
identity_compaction!(u32);
identity_compaction!(u64);
identity_compaction!(u128);

impl<const S: usize, T: Compactable + Clone> Compactable for [T; S] {
    type Compact = Self;

    fn count(&self) -> CountPerTable {
        let mut count = CountPerTable::default();
        for item in self.iter() {
            count += item.count();
        }
        count
    }

    fn compact<R>(&self, _ctx: &mut CompactionContext<R>) -> Self::Compact
    where
        R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
    {
        self.clone()
    }

    fn decompact<R>(compact: Self::Compact, _reg: &R) -> Self
    where
        R: db::RegistryRead,
    {
        compact
    }
}

impl<T: Compactable + Clone> Compactable for Vec<T> {
    type Compact = Self;

    fn count(&self) -> CountPerTable {
        let mut count = CountPerTable::default();
        for item in self.iter() {
            count += item.count();
        }
        count
    }

    fn compact<R>(&self, _ctx: &mut CompactionContext<R>) -> Self::Compact
    where
        R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
    {
        self.clone()
    }

    fn decompact<R>(compact: Self::Compact, _reg: &R) -> Self
    where
        R: db::RegistryRead,
    {
        compact
    }
}

impl<T> Compactable for PhantomData<T> {
    type Compact = ();

    fn count(&self) -> CountPerTable {
        CountPerTable::default()
    }

    fn compact<R>(&self, _ctx: &mut CompactionContext<R>) -> Self::Compact
    where
        R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
    {
        ()
    }

    fn decompact<R>(_compact: Self::Compact, _reg: &R) -> Self
    where
        R: db::RegistryRead,
    {
        Self
    }
}

#[cfg(test)]
mod tests {
    use fuel_core_types::fuel_types::Address;

    use crate::{
        registry::{
            db,
            in_memory::InMemoryRegistry,
            tables,
            CountPerTable,
        },
        Key,
    };

    use super::{
        Compactable,
        CompactionContext,
    };

    impl Compactable for Address {
        type Compact = Key<tables::Address>;

        fn count(&self) -> crate::registry::CountPerTable {
            CountPerTable {
                Address: 1,
                ..Default::default()
            }
        }

        fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
        where
            R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
        {
            ctx.to_key::<tables::Address>(**self)
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            Address::from(reg.read::<tables::Address>(compact))
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ManualExample {
        a: Address,
        b: Address,
        c: u64,
    }

    #[derive(Debug, PartialEq)]
    struct ManualExampleCompact {
        a: Key<tables::Address>,
        b: Key<tables::Address>,
        c: u64,
    }

    impl Compactable for ManualExample {
        type Compact = ManualExampleCompact;

        fn count(&self) -> crate::registry::CountPerTable {
            CountPerTable {
                Address: 2,
                ..Default::default()
            }
        }

        fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
        where
            R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
        {
            let a = ctx.to_key::<tables::Address>(*self.a);
            let b = ctx.to_key::<tables::Address>(*self.b);
            ManualExampleCompact { a, b, c: self.c }
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            let a = Address::from(reg.read::<tables::Address>(compact.a));
            let b = Address::from(reg.read::<tables::Address>(compact.b));
            Self { a, b, c: compact.c }
        }
    }

    fn check<C: Clone + Compactable + PartialEq + std::fmt::Debug>(target: C)
    where
        C::Compact: std::fmt::Debug,
    {
        let mut registry = InMemoryRegistry::default();

        let compacted = CompactionContext::run(&mut registry, target.clone());
        let decompacted = C::decompact(compacted, &registry);
        assert_eq!(target, decompacted);
    }

    #[test]
    fn test_compaction_roundtrip() {
        check(Address::default());
        check(Address::from([1u8; 32]));
        check(ManualExample {
            a: Address::from([1u8; 32]),
            b: Address::from([2u8; 32]),
            c: 3,
        });
    }
}
