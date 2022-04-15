use serde::{Deserialize, Serialize};
use std::io;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum GossipsubMessage {
    BroadcastNewTx,
    BroadcastNewBlock,
}

impl GossipsubMessage {
    pub fn encode(&self) -> Result<Vec<u8>, io::Error> {
        match bincode::serialize(&self) {
            Ok(encoded_data) => Ok(encoded_data),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    }

    pub fn decode(encoded_data: &[u8]) -> Result<Self, io::Error> {
        match bincode::deserialize(encoded_data) {
            Ok(decoded_data) => Ok(decoded_data),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    }
}
