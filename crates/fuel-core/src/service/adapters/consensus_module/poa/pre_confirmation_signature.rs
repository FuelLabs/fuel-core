use fuel_core_types::{
    fuel_tx::TxId,
    services::p2p::PreconfirmationStatus,
};

pub mod broadcast;
pub mod key_generator;
pub mod parent_signature;
pub mod signing_key;
pub mod trigger;
pub mod tx_receiver;

pub type Preconfirmations = Vec<(TxId, PreconfirmationStatus)>;
