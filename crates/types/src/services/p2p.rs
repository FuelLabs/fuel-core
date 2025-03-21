//! Contains types related to P2P data

#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

#[cfg(feature = "serde")]
use super::txpool::PoolTransaction;
use super::{
    preconfirmation::Preconfirmations,
    txpool::ArcPoolTx,
};
#[cfg(feature = "test-helpers")]
use crate::services::preconfirmation::PreconfirmationStatus;
use crate::{
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};
use std::{
    collections::HashSet,
    fmt::{
        Debug,
        Display,
        Formatter,
    },
    str::FromStr,
    time::SystemTime,
};

pub use tai64::Tai64;

/// Contains types and logic for Peer Reputation
pub mod peer_reputation;

/// The type of the public key used for signing pre-confirmations
pub type DelegatePublicKey = ed25519_dalek::VerifyingKey;

/// The type of the signature used by the block producer to sign delegation.
pub type ProtocolSignature = crate::fuel_crypto::Signature;

/// List of transactions
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transactions(pub Vec<Transaction>);

/// Lightweight representation of gossipped data that only includes IDs
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GossipsubMessageInfo {
    /// The message id that corresponds to a message payload (typically a unique hash)
    pub message_id: Vec<u8>,
    /// The ID of the network peer that sent this message
    pub peer_id: PeerId,
}

/// Status of a message received via Gossip
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum GossipsubMessageAcceptance {
    /// The gossiped message is valid and safe to rebroadcast.
    Accept,
    /// The gossiped message is invalid and should be ignored.
    Ignore,
    /// The gossiped message is invalid and anyone relaying it should be penalized.
    Reject,
}

/// A gossipped message from the network containing all relevant data.
// TODO: `T` should be inside of the `Arc` since vi broadcast it via the channel.
#[derive(Debug, Clone)]
pub struct GossipData<T> {
    /// The gossipped message payload
    /// This is meant to be consumed once to avoid cloning. Subsequent attempts to fetch data from
    /// the message should return None.
    pub data: Option<T>,
    /// The ID of the network peer that sent this message
    pub peer_id: PeerId,
    /// The message id that corresponds to a message payload (typically a unique hash)
    pub message_id: Vec<u8>,
}

/// Transactions gossiped by peers for inclusion into a block
pub type TransactionGossipData = GossipData<Transaction>;

/// A value and an associated signature
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sealed<Entity, S> {
    /// The actual value
    pub entity: Entity,
    /// Seal
    pub signature: S,
}

/// A key that will be used to sign a pre-confirmations
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DelegatePreConfirmationKey<P> {
    /// The public key of the person who is allowed to create pre-confirmations.
    pub public_key: P,
    /// The time at which the key will expire. Used to indicate to the recipient which key
    /// to use to verify the pre-confirmations--serves the second purpose of being a nonce of
    /// each key
    pub expiration: Tai64,
}

/// A signed key delegation
pub type SignedByBlockProducerDelegation<P, S> = Sealed<DelegatePreConfirmationKey<P>, S>;

/// A signed pre-confirmation
pub type SignedPreconfirmationByDelegate<S> = Sealed<Preconfirmations, S>;

/// The possible messages sent by the parties pre-confirming transactions
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreConfirmationMessage<DP, DS, S> {
    /// Notification of key delegation
    Delegate {
        /// The sealed key delegation.
        seal: SignedByBlockProducerDelegation<DP, S>,
        /// The nonce of the p2p message to make it unique.
        nonce: u64,
    },
    /// Notification of pre-confirmations
    Preconfirmations(SignedPreconfirmationByDelegate<DS>),
}

#[cfg(feature = "test-helpers")]
impl<DP, S> PreConfirmationMessage<DP, crate::fuel_tx::Bytes64, S> {
    /// Test helper for creating arbitrary, meaningless `TxConfirmations` data
    pub fn default_test_confirmation() -> Self {
        use crate::{
            fuel_tx::TxId,
            services::preconfirmation::Preconfirmation,
        };
        Self::Preconfirmations(SignedPreconfirmationByDelegate {
            entity: Preconfirmations {
                expiration: Tai64::UNIX_EPOCH,
                preconfirmations: vec![Preconfirmation {
                    tx_id: TxId::default(),
                    status: PreconfirmationStatus::SqueezedOut {
                        reason: "Dummy reason".to_string(),
                    },
                }],
            },
            signature: crate::fuel_tx::Bytes64::default(),
        })
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// The source of some network data.
pub struct SourcePeer<T> {
    /// The source of the data.
    pub peer_id: PeerId,
    /// The data.
    pub data: T,
}

impl<T> SourcePeer<T> {
    /// Maps a `SourcePeer<T>` to `SourcePeer<U>` by applying a function to the
    /// contained data. The internal `peer_id` is maintained.
    pub fn map<F, U>(self, mut f: F) -> SourcePeer<U>
    where
        F: FnMut(T) -> U,
    {
        let peer_id = self.peer_id;
        let data = f(self.data);
        SourcePeer { peer_id, data }
    }
}

impl<T> GossipData<T> {
    /// Construct a new gossip message
    pub fn new(
        data: T,
        peer_id: impl Into<Vec<u8>>,
        message_id: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            data: Some(data),
            peer_id: PeerId::from(peer_id.into()),
            message_id: message_id.into(),
        }
    }
}

/// A generic representation of data that's been gossipped by the network
pub trait NetworkData<T>: Debug + Send {
    /// Consume ownership of data from a gossipped message
    fn take_data(&mut self) -> Option<T>;
}

impl<T: Debug + Send + 'static> NetworkData<T> for GossipData<T> {
    fn take_data(&mut self) -> Option<T> {
        self.data.take()
    }
}
/// Used for relying latest `BlockHeight` info from connected peers
#[derive(Debug, Clone)]
pub struct BlockHeightHeartbeatData {
    /// PeerId as bytes
    pub peer_id: PeerId,
    /// Latest BlockHeight received
    pub block_height: BlockHeight,
}

/// Opaque peer identifier.
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PeerId(Vec<u8>);

impl AsRef<[u8]> for PeerId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for PeerId {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<PeerId> for Vec<u8> {
    fn from(peer_id: PeerId) -> Self {
        peer_id.0
    }
}

impl Display for PeerId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&bs58::encode(&self.0).into_string())
    }
}

impl FromStr for PeerId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = bs58::decode(s).into_vec().map_err(|e| e.to_string())?;
        Ok(Self(bytes))
    }
}

impl PeerId {
    /// Bind the PeerId and given data of type T together to generate a
    /// `SourcePeer<T>`
    pub fn bind<T>(self, data: T) -> SourcePeer<T> {
        SourcePeer {
            peer_id: self,
            data,
        }
    }
}

/// Contains metadata about a connected peer
pub struct PeerInfo {
    /// The libp2p peer id
    pub id: PeerId,
    /// all known multi-addresses of the peer
    pub peer_addresses: HashSet<String>,
    /// the version of fuel-core reported by the peer
    pub client_version: Option<String>,
    /// recent heartbeat from the peer
    pub heartbeat_data: HeartbeatData,
    /// the current application reputation score of the peer
    pub app_score: f64,
}

/// Contains information from the most recent heartbeat received by the peer
pub struct HeartbeatData {
    /// The currently reported block height of the peer
    pub block_height: Option<BlockHeight>,
    /// The instant representing when the latest heartbeat was received.
    pub last_heartbeat: SystemTime,
}

/// Type that represents the networkable transaction pool
/// It serializes from an Arc pool transaction and deserializes to a transaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkableTransactionPool {
    /// A transaction pool transaction
    PoolTransaction(ArcPoolTx),
    /// A transaction
    Transaction(Transaction),
}

#[cfg(feature = "serde")]
/// Serialize only the pool transaction variant
impl Serialize for NetworkableTransactionPool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use crate::fuel_tx::{
            Blob,
            Create,
            Script,
            Upgrade,
            Upload,
        };

        #[cfg(all(debug_assertions, feature = "test-helpers"))]
        // When a new variant is added to `Transaction`, the `TransactionRef`
        // must also be updated to match the new variant.
        // This match statement will trigger a compilation error if a new variant is added.
        // Don't add a `_` wildcard to this match statement, instead handle new variant.
        {
            match Transaction::default_test_tx() {
                Transaction::Script(_) => {}
                Transaction::Create(_) => {}
                Transaction::Mint(_) => {}
                Transaction::Upgrade(_) => {}
                Transaction::Upload(_) => {}
                Transaction::Blob(_) => {}
            }
        }

        #[derive(serde::Serialize)]
        enum TransactionRef<'a> {
            Script(&'a Script),
            Create(&'a Create),
            #[allow(dead_code)]
            Mint,
            Upgrade(&'a Upgrade),
            Upload(&'a Upload),
            Blob(&'a Blob),
        }

        match self {
            NetworkableTransactionPool::PoolTransaction(tx) => match (*tx).as_ref() {
                PoolTransaction::Script(tx, _) => {
                    TransactionRef::Script(tx.transaction()).serialize(serializer)
                }
                PoolTransaction::Create(tx, _) => {
                    TransactionRef::Create(tx.transaction()).serialize(serializer)
                }
                PoolTransaction::Blob(tx, _) => {
                    TransactionRef::Blob(tx.transaction()).serialize(serializer)
                }
                PoolTransaction::Upgrade(tx, _) => {
                    TransactionRef::Upgrade(tx.transaction()).serialize(serializer)
                }
                PoolTransaction::Upload(tx, _) => {
                    TransactionRef::Upload(tx.transaction()).serialize(serializer)
                }
            },
            NetworkableTransactionPool::Transaction(tx) => tx.serialize(serializer),
        }
    }
}

#[cfg(feature = "serde")]
/// Deserialize to a transaction variant
impl<'de> Deserialize<'de> for NetworkableTransactionPool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(NetworkableTransactionPool::Transaction(
            Transaction::deserialize(deserializer)?,
        ))
    }
}

impl TryFrom<NetworkableTransactionPool> for Transaction {
    type Error = &'static str;

    fn try_from(value: NetworkableTransactionPool) -> Result<Self, Self::Error> {
        match value {
            NetworkableTransactionPool::Transaction(tx) => Ok(tx),
            _ => Err("Cannot convert to transaction"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        fuel_tx::Transaction,
        services::p2p::NetworkableTransactionPool,
    };

    #[test]
    fn ser_der() {
        // Given
        let transaction = Transaction::default_test_tx();
        let expected = NetworkableTransactionPool::Transaction(transaction.clone());
        let bytes = postcard::to_allocvec(&expected).unwrap();

        // When
        let actual: NetworkableTransactionPool = postcard::from_bytes(&bytes).unwrap();

        // Then
        assert_eq!(actual, expected);
    }
}
