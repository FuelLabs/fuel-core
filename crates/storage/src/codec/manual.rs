//! The module contains the implementation of the `Manual` codec.
//! The codec allows the definition of manual implementation for specific
//! types that don't follow any patterns from other codecs. Anyone can implement
//! a codec like that, and it's more of an example of how it can be done for foreign types.

use crate::codec::{
    Decode,
    Encode,
};
use fuel_core_types::fuel_vm::{
    ContractsAssetKey,
    ContractsStateKey,
};
use std::borrow::Cow;

/// The codec allows the definition of manual implementation for specific type `T`.
pub struct Manual<T>(core::marker::PhantomData<T>);

// TODO: Use `Raw` instead of `Manual` for `ContractsAssetKey`, `ContractsStateKey`, and `OwnedMessageKey`
//  when `double_key` macro will generate `TryFrom<&[u8]>` implementation.

impl Encode<ContractsAssetKey> for Manual<ContractsAssetKey> {
    type Encoder<'a> = Cow<'a, [u8]>;

    fn encode(t: &ContractsAssetKey) -> Self::Encoder<'_> {
        Cow::Borrowed(t.as_ref())
    }
}

impl Decode<ContractsAssetKey> for Manual<ContractsAssetKey> {
    fn decode(bytes: &[u8]) -> anyhow::Result<ContractsAssetKey> {
        ContractsAssetKey::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("Unable to decode bytes"))
    }
}

impl Encode<ContractsStateKey> for Manual<ContractsStateKey> {
    type Encoder<'a> = Cow<'a, [u8]>;

    fn encode(t: &ContractsStateKey) -> Self::Encoder<'_> {
        Cow::Borrowed(t.as_ref())
    }
}

impl Decode<ContractsStateKey> for Manual<ContractsStateKey> {
    fn decode(bytes: &[u8]) -> anyhow::Result<ContractsStateKey> {
        ContractsStateKey::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("Unable to decode bytes"))
    }
}
