use crate::{
    registry::{
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
    reg: &'a mut R,
    next_keys: KeyPerTable,
    key_limits: CountPerTable,
    changes: ChangesPerTable,
}
impl<'a, R> CompactionContext<'a, R>
where
    R: db::RegistrySelectNextKey,
{
    pub fn new<C: Compactable>(reg: &'a mut R, target: &C) -> Self {
        let next_keys = next_keys(reg);
        let key_limits = target.count();

        Self {
            reg,
            next_keys,
            key_limits,
            changes: Default::default(),
        }
    }
}

impl<'a, R> CompactionContext<'a, R>
where
    R: db::RegistryRead + db::RegistryIndex,
{
    pub fn compact<T: Table>(&mut self, value: T::Type) -> Key<T> {
        // Check if the registry contains this value already
        if let Some(key) = self.reg.index_lookup::<T>(&value) {
            let limit = self.key_limits.by_table::<T>();
            // Check if the value is in the possibly-overwritable range
            if false {
                // TODO
                return key;
            }
        }
        // Allocate a new key for this
        let key = self.next_keys.mut_by_table::<T>().take_next();
        self.changes.push::<T>(value);
        key
    }
}

impl<'a, R> CompactionContext<'a, R>
where
    R: db::RegistryWrite,
{
    /// Apply all changes to the registry
    pub fn apply(self) {
        self.changes.apply(self.reg);
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
            ctx.compact::<tables::Address>(**self)
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            Address::from(reg.read::<tables::Address>(compact))
        }
    }

    #[derive(Debug, PartialEq)]
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
            let a = ctx.compact::<tables::Address>(*self.a);
            let b = ctx.compact::<tables::Address>(*self.b);
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

    fn check<C: Compactable + PartialEq + std::fmt::Debug>(target: C)
    where
        C::Compact: std::fmt::Debug,
    {
        let mut registry = InMemoryRegistry::default();

        let key_counts = target.count();
        let mut ctx = CompactionContext::new(&mut registry, &target);
        let compacted = target.compact(&mut ctx);
        dbg!(&registry);
        dbg!(&compacted);
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
