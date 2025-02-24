use std::{
    array::TryFromSliceError,
    fmt::Display,
    str::FromStr,
};

use async_graphql::{
    EmptyMutation,
    EmptySubscription,
    InputValueError,
    ScalarType,
};
use hex::FromHexError;

use crate::ports;

/// The schema of the state root service.
pub type Schema<Storage> =
    async_graphql::Schema<Query<Storage>, EmptyMutation, EmptySubscription>;

/// The query type of the schema.
pub struct Query<Storage> {
    storage: Storage,
}

impl<Storage> Query<Storage> {
    /// Create a new query object.
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }
}

#[async_graphql::Object]
impl<Storage> Query<Storage>
where
    Storage: ports::GetStateRoot + Send + Sync,
{
    async fn state_root(
        &self,
        height: BlockHeight,
    ) -> async_graphql::Result<Option<MerkleRoot>> {
        let state_root = self.storage.state_root_at(height.0.into())?;

        Ok(state_root.map(|root| MerkleRoot(*root)))
    }
}

/// GraphQL scalar type for block height.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::FromStr,
    derive_more::Display,
)]
pub struct BlockHeight(u32);

/// GraphQL scalar type for the merkle root.
#[derive(Clone, Copy, Debug)]
pub struct MerkleRoot([u8; 32]);

impl Display for MerkleRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl FromStr for MerkleRoot {
    type Err = MerkleRootParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s.trim_start_matches("0x"))?;
        Ok(MerkleRoot(bytes.as_slice().try_into()?))
    }
}

/// Error type for the merkle root parsing.
#[derive(Clone, Debug, derive_more::Display, derive_more::From, derive_more::Error)]
pub enum MerkleRootParseError {
    /// The merkle root isn't valid hex.
    DecodeFailure(FromHexError),
    /// The merkle root has the wrong number of bytes.
    WrongLength(TryFromSliceError),
}

macro_rules! impl_scalar_type {
    ($type:ty) => {
        #[async_graphql::Scalar]
        impl ScalarType for $type {
            fn parse(
                value: async_graphql::Value,
            ) -> async_graphql::InputValueResult<Self> {
                let async_graphql::Value::String(text) = value else {
                    return Err(InputValueError::expected_type(value))
                };

                text.parse().map_err(InputValueError::custom)
            }

            fn to_value(&self) -> async_graphql::Value {
                async_graphql::Value::String(self.to_string())
            }
        }
    };
}

impl_scalar_type!(MerkleRoot);
impl_scalar_type!(BlockHeight);
