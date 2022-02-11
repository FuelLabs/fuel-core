use crate::client::schema::{schema, HexString256};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Account {
    pub address: HexString256,
}
