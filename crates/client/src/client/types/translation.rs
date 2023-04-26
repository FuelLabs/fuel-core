use crate::client::{
    pagination::PaginatedResult,
    schema,
    types::{
        block::{
            Genesis,
            Header,
            PoAConsensus,
        },
        message::{
            Message,
            MessageProof,
        },
        primitives::{
            Bytes32,
            Bytes64,
            BytesN,
        },
        scalars::TransactionId,
        Block,
        Consensus,
        MerkleProof,
    },
};

impl From<schema::block::Header> for Header {
    fn from(value: schema::block::Header) -> Self {
        let id: Bytes32 = value.id.0 .0.into();
        let transactions_root: Bytes32 = value.transactions_root.0 .0.into();
        let message_receipt_root: Bytes32 = value.message_receipt_root.0 .0.into();
        let prev_root: Bytes32 = value.prev_root.0 .0.into();
        let application_hash: Bytes32 = value.application_hash.0 .0.into();

        Self {
            id: id.into(),
            da_height: value.da_height.0.into(),
            transactions_count: value.transactions_count.0,
            message_receipt_count: value.message_receipt_count.0,
            transactions_root: transactions_root.into(),
            message_receipt_root: message_receipt_root.into(),
            height: value.height.0.into(),
            prev_root: prev_root.into(),
            time: value.time.0.into(),
            application_hash,
        }
    }
}

impl From<schema::block::Consensus> for Consensus {
    fn from(value: schema::block::Consensus) -> Self {
        match value {
            schema::block::Consensus::Genesis(genesis) => {
                Consensus::Genesis(genesis.into())
            }
            schema::block::Consensus::PoAConsensus(poa) => {
                Consensus::PoAConsensus(poa.into())
            }
            schema::block::Consensus::Unknown => Consensus::Unknown,
        }
    }
}

impl From<schema::block::Genesis> for Genesis {
    fn from(value: schema::block::Genesis) -> Self {
        let chain_config_hash: Bytes32 = value.chain_config_hash.0 .0.into();
        let coins_root: Bytes32 = value.coins_root.0 .0.into();
        let contracts_root: Bytes32 = value.coins_root.0 .0.into();
        let messages_root: Bytes32 = value.coins_root.0 .0.into();
        Self {
            chain_config_hash,
            coins_root: coins_root.into(),
            contracts_root: contracts_root.into(),
            messages_root: messages_root.into(),
        }
    }
}

impl From<schema::block::PoAConsensus> for PoAConsensus {
    fn from(value: schema::block::PoAConsensus) -> Self {
        let signature: Bytes64 = value.signature.0 .0.into();
        Self {
            signature: signature.into(),
        }
    }
}

impl From<schema::block::Block> for Block {
    fn from(value: schema::block::Block) -> Self {
        let id: Bytes32 = value.id.0 .0.into();
        let transactions = value
            .transactions
            .iter()
            .map(|tx| tx.id.0 .0)
            .map(Into::<Bytes32>::into)
            .map(Into::into)
            .collect::<Vec<TransactionId>>();
        let block_producer = value.block_producer().map(|key| {
            let bytes: Bytes64 = key.into();
            bytes.into()
        });
        Self {
            id: id.into(),
            header: value.header.into(),
            consensus: value.consensus.into(),
            transactions,
            block_producer,
        }
    }
}

impl From<schema::block::BlockConnection> for PaginatedResult<Block, String> {
    fn from(conn: schema::block::BlockConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node.into()).collect(),
        }
    }
}

impl From<schema::message::Message> for Message {
    fn from(value: schema::message::Message) -> Self {
        let sender: Bytes32 = value.sender.0 .0.into();
        let recipient: Bytes32 = value.recipient.0 .0.into();
        let nonce: Bytes32 = value.nonce.0 .0.into();
        let data: BytesN = value.data.0 .0.into();
        Self {
            amount: value.amount.0,
            sender: sender.into(),
            recipient: recipient.into(),
            nonce: nonce.into(),
            data: data.into(),
            da_height: value.da_height.0,
        }
    }
}

impl From<schema::message::MessageConnection> for PaginatedResult<Message, String> {
    fn from(conn: schema::message::MessageConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node.into()).collect(),
        }
    }
}

impl From<schema::message::MessageProof> for MessageProof {
    fn from(value: schema::message::MessageProof) -> Self {
        let message_proof = value.message_proof.into();
        let block_proof = value.block_proof.into();
        let message_block_header = value.message_block_header.into();
        let commit_block_header = value.commit_block_header.into();
        let sender: Bytes32 = value.sender.0 .0.into();
        let recipient: Bytes32 = value.recipient.0 .0.into();
        let nonce: Bytes32 = value.nonce.0 .0.into();
        let amount = value.amount.0;
        let data: BytesN = value.data.0 .0.into();

        Self {
            message_proof,
            block_proof,
            message_block_header,
            commit_block_header,
            sender: sender.into(),
            recipient: recipient.into(),
            nonce: nonce.into(),
            amount,
            data: data.into(),
        }
    }
}

impl From<schema::message::MerkleProof> for MerkleProof {
    fn from(value: schema::message::MerkleProof) -> Self {
        let proof_set = value
            .proof_set
            .iter()
            .map(|v| v.0 .0)
            .map(Into::<Bytes32>::into)
            .map(Into::into)
            .collect::<Vec<_>>();
        let proof_index = value.proof_index.into();
        Self {
            proof_set,
            proof_index,
        }
    }
}
