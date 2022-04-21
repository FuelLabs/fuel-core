use serde::{Deserialize, Serialize};
use std::io;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum GossipsubMessage {
    BroadcastNewTx,
    BroadcastNewBlock,
}

impl GossipsubMessage {
    pub fn encode<T: serde::Serializer<Ok = ()>>(&self, serializer: T) -> Result<(), io::Error> {
        serde::Serialize::serialize(self, serializer)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    pub fn decode<'a, T: serde::Deserializer<'a>>(deserializer: T) -> Result<Self, io::Error> {
        serde::Deserialize::deserialize(deserializer)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
}
