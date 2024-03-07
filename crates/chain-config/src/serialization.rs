use serde::{
    Deserializer,
    Serializer,
};
use serde_with::{
    formats::Lowercase,
    DeserializeAs,
    SerializeAs,
};

/// Encode/decode a byte vector as a hex string if the serializer is human readable. Otherwise, use the default encoding.
pub(crate) struct HexIfHumanReadable;

impl SerializeAs<Vec<u8>> for HexIfHumanReadable {
    fn serialize_as<S>(value: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serde_with::hex::Hex::<Lowercase>::serialize_as(value, serializer)
        } else {
            Ok(serde::Serialize::serialize(value, serializer)?)
        }
    }
}

impl<'a> DeserializeAs<'a, Vec<u8>> for HexIfHumanReadable {
    fn deserialize_as<D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'a>,
    {
        if deserializer.is_human_readable() {
            // Per docs: doesn't care about lower/upper case, decodes even mixed case.
            serde_with::hex::Hex::<Lowercase>::deserialize_as(deserializer)
        } else {
            serde::Deserialize::deserialize(deserializer)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HexIfHumanReadable;
    use postcard::ser_flavors::{
        AllocVec,
        Flavor,
    };
    use serde_json::de::StrRead;
    use serde_with::{
        DeserializeAs,
        SerializeAs,
    };

    const BYTES: [u8; 32] = [1u8; 32];
    const HUMAN_READABLE_FORM: &str =
        "\"0101010101010101010101010101010101010101010101010101010101010101\"";

    #[test]
    fn encodes_hex_type_as_human_readable() {
        // given
        let value = Vec::from(BYTES);
        let mut buf = vec![];
        let mut serializer = serde_json::Serializer::new(&mut buf);

        // when
        <HexIfHumanReadable as SerializeAs<Vec<u8>>>::serialize_as(
            &value,
            &mut serializer,
        )
        .unwrap();

        // then
        let actual = String::from_utf8(buf).unwrap();
        assert_eq!(actual, HUMAN_READABLE_FORM);
    }

    #[test]
    fn decodes_hex_type_as_human_readable() {
        // given
        let serialized = HUMAN_READABLE_FORM;
        let mut deserializer = serde_json::Deserializer::new(StrRead::new(serialized));

        // when
        let decoded = <HexIfHumanReadable as DeserializeAs<Vec<u8>>>::deserialize_as(
            &mut deserializer,
        )
        .unwrap();

        // then
        let expectation = Vec::from(BYTES);
        assert_eq!(decoded, expectation);
    }

    #[test]
    fn encodes_hex_type_as_machine_readable() {
        // given
        let mut buf = [0u8; 33];
        let expected_bytes = postcard::to_slice(&BYTES.as_slice(), &mut buf).unwrap();
        let mut serializer = postcard::Serializer {
            output: AllocVec::default(),
        };

        // when
        <HexIfHumanReadable as SerializeAs<Vec<u8>>>::serialize_as(
            &BYTES.to_vec(),
            &mut serializer,
        )
        .unwrap();

        // then
        let bytes = serializer.output.finalize().unwrap();
        assert_eq!(bytes.as_slice(), expected_bytes);
    }

    #[test]
    fn decodes_hex_type_as_machine_readable() {
        // given
        let mut buf = [0u8; 33];
        let expected_bytes = postcard::to_slice(&BYTES.as_slice(), &mut buf).unwrap();
        let mut serializer = postcard::Deserializer::from_bytes(expected_bytes);

        // when
        let decoded = <HexIfHumanReadable as DeserializeAs<Vec<u8>>>::deserialize_as(
            &mut serializer,
        )
        .unwrap();

        // then
        assert_eq!(BYTES.to_vec(), decoded);
    }
}
