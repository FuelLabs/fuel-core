use fuel_core_storage::codec::{
    Decode,
    Encode,
};
use fuel_core_types::fuel_tx::input::PredicateCode;
use std::{
    borrow::Cow,
    ops::Deref,
};

// TODO: Remove this codec when the `PredicateCode` implements
//  `AsRef<[u8]>` and from `TryFrom<[u8]>` and use `Raw` codec instead.

pub struct PredicateCodeCodec;

impl Encode<PredicateCode> for PredicateCodeCodec {
    type Encoder<'a> = Cow<'a, [u8]>;

    fn encode(t: &PredicateCode) -> Self::Encoder<'_> {
        Cow::Borrowed(t.deref())
    }
}

impl Decode<PredicateCode> for PredicateCodeCodec {
    fn decode(bytes: &[u8]) -> anyhow::Result<PredicateCode> {
        Ok(bytes.to_vec().into())
    }
}
