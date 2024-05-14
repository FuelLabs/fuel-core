//! The module contains the implementation of the `Manual` codec.
//! The codec allows the definition of manual implementation for specific
//! types that don't follow any patterns from other codecs. Anyone can implement
//! a codec like that, and it's more of an example of how it can be done for foreign types.

/// The codec allows the definition of manual implementation for specific type `T`.
pub struct Manual<T>(core::marker::PhantomData<T>);
