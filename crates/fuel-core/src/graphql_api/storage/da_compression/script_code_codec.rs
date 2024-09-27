use fuel_core_storage::codec::{
    Decode,
    Encode,
};
use fuel_core_types::fuel_tx::ScriptCode;
use std::{
    borrow::Cow,
    ops::Deref,
};

// TODO: Remove this codec when the `ScriptCode` implements
//  `AsRef<[u8]>` and `TryFrom<[u8]>` and use `Raw` codec instead.

pub struct ScriptCodeCodec;

impl Encode<ScriptCode> for ScriptCodeCodec {
    type Encoder<'a> = Cow<'a, [u8]>;

    fn encode(t: &ScriptCode) -> Self::Encoder<'_> {
        Cow::Borrowed(t.deref())
    }
}

impl Decode<ScriptCode> for ScriptCodeCodec {
    fn decode(bytes: &[u8]) -> anyhow::Result<ScriptCode> {
        Ok(bytes.to_vec().into())
    }
}
