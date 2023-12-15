use stable_hash::{StableHash, StableHasher};
use stable_hash_legacy::prelude::{
    StableHash as StableHashLegacy, StableHasher as StableHasherLegacy,
};

/// Implements StableHash and StableHashLegacy. This macro supports two forms:
/// Struct { field1, field2, ... } and Tuple(transparent). Each field supports
/// an optional modifier. For example: Tuple(transparent: AsBytes)
#[macro_export]
macro_rules! _impl_stable_hash {
    ($T:ident$(<$lt:lifetime>)? {$($field:ident$(:$e:path)?),*}) => {
        ::stable_hash::impl_stable_hash!($T$(<$lt>)? {$($field$(:$e)?),*});
        ::stable_hash_legacy::impl_stable_hash!($T$(<$lt>)? {$($field$(:$e)?),*});
    };
    ($T:ident$(<$lt:lifetime>)? (transparent$(:$e:path)?)) => {
        ::stable_hash::impl_stable_hash!($T$(<$lt>)? (transparent$(:$e)?));
        ::stable_hash_legacy::impl_stable_hash!($T$(<$lt>)? (transparent$(:$e)?));
    };
}
pub use crate::_impl_stable_hash as impl_stable_hash;

pub struct AsBytes<T>(pub T);

impl<T> StableHashLegacy for AsBytes<T>
where
    T: AsRef<[u8]>,
{
    fn stable_hash<H: StableHasherLegacy>(&self, sequence_number: H::Seq, state: &mut H) {
        stable_hash_legacy::utils::AsBytes(self.0.as_ref()).stable_hash(sequence_number, state);
    }
}

impl<T> StableHash for AsBytes<T>
where
    T: AsRef<[u8]>,
{
    fn stable_hash<H: StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        stable_hash::utils::AsBytes(self.0.as_ref()).stable_hash(field_address, state);
    }
}
