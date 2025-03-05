use super::{
    Decode,
    Encode,
    Encoder,
    GossipsubCodec,
};
use crate::{
    gossipsub::messages::{
        GossipTopicTag,
        GossipsubBroadcastRequest,
        GossipsubMessage,
    },
    ports::P2PPreConfirmationMessage,
};
use fuel_core_types::fuel_tx::Transaction;
use std::{
    io,
    ops::Deref,
};

#[derive(Debug, Clone, Default)]
pub struct GossipsubMessageHandler<Codec> {
    pub(crate) codec: Codec,
}

impl<Codec> GossipsubCodec for GossipsubMessageHandler<Codec>
where
    Codec: Encode<Transaction, Error = io::Error>
        + Decode<Transaction, Error = io::Error>
        + Encode<P2PPreConfirmationMessage, Error = io::Error>
        + Decode<P2PPreConfirmationMessage, Error = io::Error>,
{
    type RequestMessage = GossipsubBroadcastRequest;
    type ResponseMessage = GossipsubMessage;

    fn encode(&self, data: Self::RequestMessage) -> Result<Vec<u8>, io::Error> {
        match data {
            GossipsubBroadcastRequest::NewTx(tx) => {
                Ok(self.codec.encode(tx.deref())?.into_bytes())
            }
            GossipsubBroadcastRequest::TxPreConfirmations(msg) => {
                Ok(self.codec.encode(msg.deref())?.into_bytes())
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
                GossipsubMessage::NewTx(self.codec.decode(encoded_data)?)
            }
            GossipTopicTag::TxPreConfirmations => {
                GossipsubMessage::TxPreConfirmations(self.codec.decode(encoded_data)?)
            }
        };

        Ok(decoded_response)
    }
}
