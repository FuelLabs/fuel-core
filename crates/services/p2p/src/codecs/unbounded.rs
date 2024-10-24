use std::{
    io,
    marker::PhantomData,
};

use crate::gossipsub::messages::{
    GossipTopicTag,
    GossipsubBroadcastRequest,
    GossipsubMessage,
};

use super::{
    DataFormatCodec,
    GossipsubCodec,
};

#[derive(Debug, Clone)]
pub struct UnboundedCodec<Format> {
    _format: PhantomData<Format>,
}

impl<Format> UnboundedCodec<Format> {
    pub fn new() -> Self {
        UnboundedCodec {
            _format: PhantomData,
        }
    }
}

impl<Format> GossipsubCodec for UnboundedCodec<Format>
where
    Format: DataFormatCodec<Error = io::Error> + Send,
{
    type RequestMessage = GossipsubBroadcastRequest;
    type ResponseMessage = GossipsubMessage;

    fn encode(&self, data: Self::RequestMessage) -> Result<Vec<u8>, io::Error> {
        match data {
            GossipsubBroadcastRequest::NewTx(tx) => Format::serialize(&*tx),
        }
    }

    fn decode(
        &self,
        encoded_data: &[u8],
        gossipsub_tag: GossipTopicTag,
    ) -> Result<Self::ResponseMessage, io::Error> {
        let decoded_response = match gossipsub_tag {
            GossipTopicTag::NewTx => {
                GossipsubMessage::NewTx(Format::deserialize(encoded_data)?)
            }
        };

        Ok(decoded_response)
    }
}
