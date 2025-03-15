#![allow(non_snake_case)]

mod mocks;
mod tests_e2e;
mod tests_permits;
mod tests_sending;
mod tests_service;
mod tests_subscribe;
mod tests_update_stream_state;
mod universe;
mod utils;

use crate::service::ProtocolPublicKey;
use fuel_core_types::{
    fuel_crypto::PublicKey,
    fuel_tx::{
        Address,
        Input,
    },
};

impl ProtocolPublicKey for PublicKey {
    fn latest_address(&self) -> Address {
        Input::owner(self)
    }
}
