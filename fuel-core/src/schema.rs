pub mod block;
pub mod dap;
pub mod health;
pub mod tx;

use async_graphql::{
    EmptySubscription, InputType, InputValueError, InputValueResult, MergedObject, Scalar,
    ScalarType, Schema, SchemaBuilder, Value,
};
use std::convert::TryInto;

#[derive(MergedObject, Default)]
pub struct Query(
    dap::DapQuery,
    block::BlockQuery,
    tx::TxQuery,
    health::HealthQuery,
);

#[derive(MergedObject, Default)]
pub struct Mutation(dap::DapMutation, tx::TxMutation);

// Placeholder for when we need to add subscriptions
// #[derive(MergedSubscription, Default)]
// pub struct Subscription();

pub type CoreSchema = Schema<Query, Mutation, EmptySubscription>;

pub fn build_schema() -> SchemaBuilder<Query, Mutation, EmptySubscription> {
    Schema::build(
        Query::default(),
        Mutation::default(),
        EmptySubscription::default(),
    )
}

pub struct HexString256([u8; 32]);

#[Scalar(name = "HexString256")]
impl ScalarType for HexString256 {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            // trim leading 0x
            let value = value
                .strip_prefix("0x")
                .ok_or(InputValueError::custom("expected 0x prefix"))?;
            // decode into bytes
            let mut bytes = ((value.len() / 2)..32).map(|_| 0).collect::<Vec<u8>>();
            // pad input to 32 bytes
            bytes.extend(hex::decode(value)?);
            // attempt conversion to fixed length array, error if too long
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|e: Vec<u8>| format!("expected 32 bytes, received {}", e.len()))?;

            Ok(HexString256(bytes))
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(format!("0x{}", hex::encode(self.0)))
    }
}
