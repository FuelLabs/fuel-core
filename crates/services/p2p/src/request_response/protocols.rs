//! This module contains structs and functions for versioning
//!  request response protocols, and for recovering the list
//!  of different versions of the protocol understood by
//!  connected peers.

use libp2p::{
    identify,
    StreamProtocol,
};

use crate::codecs::postcard::MessageExchangePostcardProtocol;

use super::messages::REQUEST_RESPONSE_PROTOCOL_ID;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProtocolVersion {
    V1(MessageExchangePostcardProtocol),
}

impl ProtocolVersion {
    pub fn as_ref(&self) -> &str {
        match self {
            ProtocolVersion::V1(protocol) => protocol.as_ref(),
        }
    }

    pub fn latest_compatible_version_for_peer(
        info: identify::Info,
    ) -> Option<ProtocolVersion> {
        info.protocols
            .iter()
            .filter_map(|protocol| ProtocolVersion::try_from(protocol.clone()).ok())
            .max()
    }
}

impl TryFrom<StreamProtocol> for ProtocolVersion {
    // TODO: Better error type
    type Error = ();

    fn try_from(protocol: StreamProtocol) -> Result<Self, Self::Error> {
        match protocol.as_ref() {
            REQUEST_RESPONSE_PROTOCOL_ID => {
                Ok(ProtocolVersion::V1(MessageExchangePostcardProtocol))
            }
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use libp2p::{
        identify::{
            self,
        },
        identity::PublicKey,
        Multiaddr,
        StreamProtocol,
    };

    use crate::{
        codecs::postcard::MessageExchangePostcardProtocol,
        heartbeat::HEARTBEAT_PROTOCOL,
        request_response::protocols::ProtocolVersion,
    };

    fn peer_info<'a>(protocols: &[impl AsRef<str>]) -> identify::Info {
        let public_key = PublicKey::try_decode_protobuf(&hex::decode(
            "080112201ed1e8fae2c4a144b8be8fd4b47bf3d3b34b871c3cacf6010f0e42d474fce27e",
        ).unwrap()).unwrap();

        let mut stream_protocols: Vec<StreamProtocol> =
            Vec::with_capacity(protocols.len());
        for protocol in protocols {
            stream_protocols.push(
                StreamProtocol::try_from_owned(protocol.as_ref().to_string()).unwrap(),
            );
        }

        identify::Info {
            protocols: stream_protocols,
            agent_version: "0.0.1".to_string(),
            protocol_version: "0.0.1".to_string(),
            public_key,
            listen_addrs: vec![],
            observed_addr: Multiaddr::empty(),
        }
    }

    #[test]
    fn test_latest_protocol_version_defined() {
        let peer_info =
            peer_info(&[MessageExchangePostcardProtocol.as_ref(), HEARTBEAT_PROTOCOL]);
        let latest_compatible_version_for_peer =
            ProtocolVersion::latest_compatible_version_for_peer(peer_info).unwrap();
        assert_eq!(
            latest_compatible_version_for_peer,
            crate::request_response::protocols::ProtocolVersion::V1(
                MessageExchangePostcardProtocol
            )
        );
    }

    #[test]
    fn test_latest_protocol_version_undefined() {
        let peer_info = peer_info(&[HEARTBEAT_PROTOCOL, "/some/other/protocol/1.0.0"]);
        let latest_compatible_version_for_peer =
            ProtocolVersion::latest_compatible_version_for_peer(peer_info);
        assert!(latest_compatible_version_for_peer.is_none(),);
    }
}
