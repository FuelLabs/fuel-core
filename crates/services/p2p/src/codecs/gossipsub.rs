use std::{
    io,
    marker::PhantomData,
};

use fuel_core_types::fuel_tx::Transaction;

use crate::gossipsub::messages::{
    GossipTopicTag,
    GossipsubBroadcastRequest,
    GossipsubMessage,
};

use super::{
    Decode,
    Encode,
    Encoder,
    GossipsubCodec,
};

#[derive(Debug, Clone)]
pub struct GossipsubMessageHandler<Format> {
    pub(crate) _data_format: PhantomData<Format>,
}

impl<Format> GossipsubMessageHandler<Format> {
    pub fn new() -> Self {
        GossipsubMessageHandler {
            _data_format: PhantomData,
        }
    }
}

impl<Format> GossipsubCodec for GossipsubMessageHandler<Format>
where
    Format: Encode<Transaction, Error = io::Error>
        + Decode<Transaction, Error = io::Error>
        + Send,
{
    type RequestMessage = GossipsubBroadcastRequest;
    type ResponseMessage = GossipsubMessage;

    fn encode(&self, data: Self::RequestMessage) -> Result<Vec<u8>, io::Error> {
        match data {
            GossipsubBroadcastRequest::NewTx(tx) => {
                Ok(Format::encode(&*tx)?.as_bytes().into_owned())
            }
        }
    }

    fn decode(
        &self,
        encoded_data: &[u8],
        gossipsub_tag: GossipTopicTag,
    ) -> Result<Self::ResponseMessage, io::Error> {
        let decoded_response = match gossipsub_tag {
            GossipTopicTag::NewTx => {
                GossipsubMessage::NewTx(Format::decode(encoded_data)?)
            }
        };

        Ok(decoded_response)
    }
}
