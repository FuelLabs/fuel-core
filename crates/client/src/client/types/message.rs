use crate::client::types::{
    block::Header,
    scalars::{
        Address,
        HexString,
        Nonce,
    },
    MerkleProof,
};

#[derive(Debug)]
pub struct Message {
    pub amount: u64,
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Nonce,
    pub data: HexString,
    pub da_height: u64,
}

#[derive(Debug)]
pub struct MessageProof {
    /// Proof that message is contained within the provided block header.
    pub message_proof: MerkleProof,
    /// Proof that the provided block header is contained within the blockchain history.
    pub block_proof: MerkleProof,
    /// The previous fuel block header that contains the message. Message block height <
    /// commit block height.
    pub message_block_header: Header,
    /// The consensus header associated with the finalized commit being used
    /// as the root of the block proof.
    pub commit_block_header: Header,
    /// The messages sender address.
    pub sender: Address,
    /// The messages recipient address.
    pub recipient: Address,
    /// The nonce from the message.
    pub nonce: Nonce,
    /// The amount from the message.
    pub amount: u64,
    /// The data from the message.
    pub data: HexString,
}
