use std::io;

use crate::gossipsub::messages::{
    GossipTopicTag,
    GossipsubBroadcastRequest,
    GossipsubMessage,
};

use super::{
    DataFormat,
    GossipsubCodec,
};

#[derive(Debug, Clone)]
pub struct UnboundedCodec<Format> {
    pub(crate) data_format: Format,
}

impl<Format> GossipsubCodec for UnboundedCodec<Format>
where
    Format: DataFormat<Error = io::Error> + Send,
{
    type RequestMessage = GossipsubBroadcastRequest;
    type ResponseMessage = GossipsubMessage;

    fn encode(&self, data: Self::RequestMessage) -> Result<Vec<u8>, io::Error> {
        match data {
            GossipsubBroadcastRequest::NewTx(tx) => self.data_format.serialize(&*tx),
        }
    }

    fn decode(
        &self,
        encoded_data: &[u8],
        gossipsub_tag: GossipTopicTag,
    ) -> Result<Self::ResponseMessage, io::Error> {
        let decoded_response = match gossipsub_tag {
            GossipTopicTag::NewTx => {
                GossipsubMessage::NewTx(self.data_format.deserialize(encoded_data)?)
            }
        };

        Ok(decoded_response)
    }
}
