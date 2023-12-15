use std::marker::PhantomData;

use super::{Blockchain, NodeCapabilities};

/// A boring implementor of [`NodeCapabilities`] for blockchains that
/// only need an empty `struct`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EmptyNodeCapabilities<C>(PhantomData<C>);

impl<C> Default for EmptyNodeCapabilities<C> {
    fn default() -> Self {
        EmptyNodeCapabilities(PhantomData)
    }
}

impl<C> std::fmt::Display for EmptyNodeCapabilities<C>
where
    C: Blockchain,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", C::KIND)
    }
}

impl<C> slog::Value for EmptyNodeCapabilities<C>
where
    C: Blockchain,
{
    fn serialize(
        &self,
        record: &slog::Record,
        key: slog::Key,
        serializer: &mut dyn slog::Serializer,
    ) -> slog::Result {
        slog::Value::serialize(&C::KIND.to_string(), record, key, serializer)
    }
}

impl<C> NodeCapabilities<C> for EmptyNodeCapabilities<C>
where
    C: Blockchain,
{
    fn from_data_sources(_data_sources: &[C::DataSource]) -> Self {
        EmptyNodeCapabilities(PhantomData)
    }
}
