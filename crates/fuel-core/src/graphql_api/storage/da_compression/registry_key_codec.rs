use fuel_core_storage::codec::{
    Decode,
    Encode,
};
use fuel_core_types::fuel_compression::RegistryKey;

// TODO: Remove this codec when the `RegisterKey` implements
//  `AsRef<[u8]>` and from `TryFrom<[u8]>` and use `Raw` codec instead.

pub struct RegistryKeyCodec;

impl Encode<RegistryKey> for RegistryKeyCodec {
    type Encoder<'a> = [u8; 4];

    fn encode(t: &RegistryKey) -> Self::Encoder<'_> {
        t.as_u32().to_be_bytes()
    }
}

impl Decode<RegistryKey> for RegistryKeyCodec {
    fn decode(bytes: &[u8]) -> anyhow::Result<RegistryKey> {
        u32::from_be_bytes(bytes.try_into()?)
            .try_into()
            .map_err(|e: &str| anyhow::anyhow!(e))
    }
}
