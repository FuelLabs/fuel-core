use graph::blockchain::Block;
use graph::blockchain::MappingTriggerTrait;
use graph::blockchain::TriggerData;
use graph::cheap_clone::CheapClone;
use graph::prelude::hex;
use graph::prelude::web3::types::H256;
use graph::prelude::BlockNumber;
use graph::runtime::HostExportError;
use graph::runtime::{asc_new, gas::GasCounter, AscHeap, AscPtr};
use graph_runtime_wasm::module::ToAscPtr;
use std::{cmp::Ordering, sync::Arc};

use crate::codec;

// Logging the block is too verbose, so this strips the block from the trigger for Debug.
impl std::fmt::Debug for NearTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[derive(Debug)]
        pub enum MappingTriggerWithoutBlock<'a> {
            Block,

            Receipt {
                outcome: &'a codec::ExecutionOutcomeWithId,
                receipt: &'a codec::Receipt,
            },
        }

        let trigger_without_block = match self {
            NearTrigger::Block(_) => MappingTriggerWithoutBlock::Block,
            NearTrigger::Receipt(receipt) => MappingTriggerWithoutBlock::Receipt {
                outcome: &receipt.outcome,
                receipt: &receipt.receipt,
            },
        };

        write!(f, "{:?}", trigger_without_block)
    }
}

impl ToAscPtr for NearTrigger {
    fn to_asc_ptr<H: AscHeap>(
        self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<()>, HostExportError> {
        Ok(match self {
            NearTrigger::Block(block) => asc_new(heap, block.as_ref(), gas)?.erase(),
            NearTrigger::Receipt(receipt) => asc_new(heap, receipt.as_ref(), gas)?.erase(),
        })
    }
}

#[derive(Clone)]
pub enum NearTrigger {
    Block(Arc<codec::Block>),
    Receipt(Arc<ReceiptWithOutcome>),
}

impl CheapClone for NearTrigger {
    fn cheap_clone(&self) -> NearTrigger {
        match self {
            NearTrigger::Block(block) => NearTrigger::Block(block.cheap_clone()),
            NearTrigger::Receipt(receipt) => NearTrigger::Receipt(receipt.cheap_clone()),
        }
    }
}

impl PartialEq for NearTrigger {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Block(a_ptr), Self::Block(b_ptr)) => a_ptr == b_ptr,
            (Self::Receipt(a), Self::Receipt(b)) => a.receipt.receipt_id == b.receipt.receipt_id,

            (Self::Block(_), Self::Receipt(_)) | (Self::Receipt(_), Self::Block(_)) => false,
        }
    }
}

impl Eq for NearTrigger {}

impl NearTrigger {
    pub fn block_number(&self) -> BlockNumber {
        match self {
            NearTrigger::Block(block) => block.number(),
            NearTrigger::Receipt(receipt) => receipt.block.number(),
        }
    }

    pub fn block_hash(&self) -> H256 {
        match self {
            NearTrigger::Block(block) => block.ptr().hash_as_h256(),
            NearTrigger::Receipt(receipt) => receipt.block.ptr().hash_as_h256(),
        }
    }

    fn error_context(&self) -> std::string::String {
        match self {
            NearTrigger::Block(..) => {
                format!("Block #{} ({})", self.block_number(), self.block_hash())
            }
            NearTrigger::Receipt(receipt) => {
                format!(
                    "receipt id {}, block #{} ({})",
                    hex::encode(&receipt.receipt.receipt_id.as_ref().unwrap().bytes),
                    self.block_number(),
                    self.block_hash()
                )
            }
        }
    }
}

impl Ord for NearTrigger {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Keep the order when comparing two block triggers
            (Self::Block(..), Self::Block(..)) => Ordering::Equal,

            // Block triggers always come last
            (Self::Block(..), _) => Ordering::Greater,
            (_, Self::Block(..)) => Ordering::Less,

            // Execution outcomes have no intrinsic ordering information, so we keep the order in
            // which they are included in the `receipt_execution_outcomes` field of `IndexerShard`.
            (Self::Receipt(..), Self::Receipt(..)) => Ordering::Equal,
        }
    }
}

impl PartialOrd for NearTrigger {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TriggerData for NearTrigger {
    fn error_context(&self) -> String {
        self.error_context()
    }

    fn address_match(&self) -> Option<&[u8]> {
        None
    }
}

impl MappingTriggerTrait for NearTrigger {
    fn error_context(&self) -> String {
        self.error_context()
    }
}

pub struct ReceiptWithOutcome {
    // REVIEW: Do we want to actually also have those two below behind an `Arc` wrapper?
    pub outcome: codec::ExecutionOutcomeWithId,
    pub receipt: codec::Receipt,
    pub block: Arc<codec::Block>,
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use super::*;

    use graph::{
        anyhow::anyhow,
        components::metrics::gas::GasMetrics,
        data::subgraph::API_VERSION_0_0_5,
        prelude::{hex, BigInt},
        runtime::{gas::GasCounter, DeterministicHostError, HostExportError},
        util::mem::init_slice,
    };

    #[test]
    fn block_trigger_to_asc_ptr() {
        let mut heap = BytesHeap::new(API_VERSION_0_0_5);
        let trigger = NearTrigger::Block(Arc::new(block()));

        let result = trigger.to_asc_ptr(&mut heap, &GasCounter::new(GasMetrics::mock()));
        assert!(result.is_ok());
    }

    #[test]
    fn receipt_trigger_to_asc_ptr() {
        let mut heap = BytesHeap::new(API_VERSION_0_0_5);
        let trigger = NearTrigger::Receipt(Arc::new(ReceiptWithOutcome {
            block: Arc::new(block()),
            outcome: execution_outcome_with_id().unwrap(),
            receipt: receipt().unwrap(),
        }));

        let result = trigger.to_asc_ptr(&mut heap, &GasCounter::new(GasMetrics::mock()));
        assert!(result.is_ok());
    }

    fn block() -> codec::Block {
        codec::Block {
            author: "test".to_string(),
            header: Some(codec::BlockHeader {
                height: 2,
                prev_height: 1,
                epoch_id: hash("01"),
                next_epoch_id: hash("02"),
                hash: hash("01"),
                prev_hash: hash("00"),
                prev_state_root: hash("bb00010203"),
                chunk_receipts_root: hash("bb00010203"),
                chunk_headers_root: hash("bb00010203"),
                chunk_tx_root: hash("bb00010203"),
                outcome_root: hash("cc00010203"),
                chunks_included: 1,
                challenges_root: hash("aa"),
                timestamp: 100,
                timestamp_nanosec: 0,
                random_value: hash("010203"),
                validator_proposals: vec![],
                chunk_mask: vec![],
                gas_price: big_int(10),
                block_ordinal: 0,
                total_supply: big_int(1_000),
                challenges_result: vec![],
                last_final_block: hash("00"),
                last_final_block_height: 0,
                last_ds_final_block: hash("00"),
                last_ds_final_block_height: 0,
                next_bp_hash: hash("bb"),
                block_merkle_root: hash("aa"),
                epoch_sync_data_hash: vec![0x00, 0x01],
                approvals: vec![],
                signature: signature("00"),
                latest_protocol_version: 0,
            }),
            chunk_headers: vec![chunk_header().unwrap()],
            shards: vec![codec::IndexerShard {
                shard_id: 0,
                chunk: Some(codec::IndexerChunk {
                    author: "near".to_string(),
                    header: chunk_header(),
                    transactions: vec![codec::IndexerTransactionWithOutcome {
                        transaction: Some(codec::SignedTransaction {
                            signer_id: "signer".to_string(),
                            public_key: public_key("aabb"),
                            nonce: 1,
                            receiver_id: "receiver".to_string(),
                            actions: vec![],
                            signature: signature("ff"),
                            hash: hash("bb"),
                        }),
                        outcome: Some(codec::IndexerExecutionOutcomeWithOptionalReceipt {
                            execution_outcome: execution_outcome_with_id(),
                            receipt: receipt(),
                        }),
                    }],
                    receipts: vec![receipt().unwrap()],
                }),
                receipt_execution_outcomes: vec![codec::IndexerExecutionOutcomeWithReceipt {
                    execution_outcome: execution_outcome_with_id(),
                    receipt: receipt(),
                }],
            }],
            state_changes: vec![],
        }
    }

    fn receipt() -> Option<codec::Receipt> {
        Some(codec::Receipt {
            predecessor_id: "genesis.near".to_string(),
            receiver_id: "near".to_string(),
            receipt_id: hash("dead"),
            receipt: Some(codec::receipt::Receipt::Action(codec::ReceiptAction {
                signer_id: "near".to_string(),
                signer_public_key: public_key("aa"),
                gas_price: big_int(2),
                output_data_receivers: vec![],
                input_data_ids: vec![],
                actions: vec![
                    codec::Action {
                        action: Some(codec::action::Action::CreateAccount(
                            codec::CreateAccountAction {},
                        )),
                    },
                    codec::Action {
                        action: Some(codec::action::Action::DeployContract(
                            codec::DeployContractAction {
                                code: vec![0x01, 0x02],
                            },
                        )),
                    },
                    codec::Action {
                        action: Some(codec::action::Action::FunctionCall(
                            codec::FunctionCallAction {
                                method_name: "func".to_string(),
                                args: vec![0x01, 0x02],
                                gas: 1000,
                                deposit: big_int(100),
                            },
                        )),
                    },
                    codec::Action {
                        action: Some(codec::action::Action::Transfer(codec::TransferAction {
                            deposit: big_int(100),
                        })),
                    },
                    codec::Action {
                        action: Some(codec::action::Action::Stake(codec::StakeAction {
                            stake: big_int(100),
                            public_key: public_key("aa"),
                        })),
                    },
                    codec::Action {
                        action: Some(codec::action::Action::AddKey(codec::AddKeyAction {
                            public_key: public_key("aa"),
                            access_key: Some(codec::AccessKey {
                                nonce: 1,
                                permission: Some(codec::AccessKeyPermission {
                                    permission: Some(
                                        codec::access_key_permission::Permission::FunctionCall(
                                            codec::FunctionCallPermission {
                                                // allowance can be None, so let's test this out here
                                                allowance: None,
                                                receiver_id: "receiver".to_string(),
                                                method_names: vec!["sayGm".to_string()],
                                            },
                                        ),
                                    ),
                                }),
                            }),
                        })),
                    },
                    codec::Action {
                        action: Some(codec::action::Action::AddKey(codec::AddKeyAction {
                            public_key: public_key("aa"),
                            access_key: Some(codec::AccessKey {
                                nonce: 1,
                                permission: Some(codec::AccessKeyPermission {
                                    permission: Some(
                                        codec::access_key_permission::Permission::FullAccess(
                                            codec::FullAccessPermission {},
                                        ),
                                    ),
                                }),
                            }),
                        })),
                    },
                    codec::Action {
                        action: Some(codec::action::Action::DeleteKey(codec::DeleteKeyAction {
                            public_key: public_key("aa"),
                        })),
                    },
                    codec::Action {
                        action: Some(codec::action::Action::DeleteAccount(
                            codec::DeleteAccountAction {
                                beneficiary_id: "suicided.near".to_string(),
                            },
                        )),
                    },
                ],
            })),
        })
    }

    fn chunk_header() -> Option<codec::ChunkHeader> {
        Some(codec::ChunkHeader {
            chunk_hash: vec![0x00],
            prev_block_hash: vec![0x01],
            outcome_root: vec![0x02],
            prev_state_root: vec![0x03],
            encoded_merkle_root: vec![0x04],
            encoded_length: 1,
            height_created: 2,
            height_included: 3,
            shard_id: 4,
            gas_used: 5,
            gas_limit: 6,
            validator_reward: big_int(7),
            balance_burnt: big_int(7),
            outgoing_receipts_root: vec![0x07],
            tx_root: vec![0x08],
            validator_proposals: vec![codec::ValidatorStake {
                account_id: "account".to_string(),
                public_key: public_key("aa"),
                stake: big_int(10),
            }],
            signature: signature("ff"),
        })
    }

    fn execution_outcome_with_id() -> Option<codec::ExecutionOutcomeWithId> {
        Some(codec::ExecutionOutcomeWithId {
            proof: Some(codec::MerklePath { path: vec![] }),
            block_hash: hash("aa"),
            id: hash("beef"),
            outcome: execution_outcome(),
        })
    }

    fn execution_outcome() -> Option<codec::ExecutionOutcome> {
        Some(codec::ExecutionOutcome {
            logs: vec!["string".to_string()],
            receipt_ids: vec![],
            gas_burnt: 1,
            tokens_burnt: big_int(2),
            executor_id: "near".to_string(),
            metadata: 0,
            status: Some(codec::execution_outcome::Status::SuccessValue(
                codec::SuccessValueExecutionStatus { value: vec![0x00] },
            )),
        })
    }

    fn big_int(input: u64) -> Option<codec::BigInt> {
        let value =
            BigInt::try_from(input).unwrap_or_else(|_| panic!("Invalid BigInt value {}", input));
        let bytes = value.to_signed_bytes_le();

        Some(codec::BigInt { bytes })
    }

    fn hash(input: &str) -> Option<codec::CryptoHash> {
        Some(codec::CryptoHash {
            bytes: hex::decode(input).unwrap_or_else(|_| panic!("Invalid hash value {}", input)),
        })
    }

    fn public_key(input: &str) -> Option<codec::PublicKey> {
        Some(codec::PublicKey {
            r#type: 0,
            bytes: hex::decode(input)
                .unwrap_or_else(|_| panic!("Invalid PublicKey value {}", input)),
        })
    }

    fn signature(input: &str) -> Option<codec::Signature> {
        Some(codec::Signature {
            r#type: 0,
            bytes: hex::decode(input)
                .unwrap_or_else(|_| panic!("Invalid Signature value {}", input)),
        })
    }

    struct BytesHeap {
        api_version: graph::semver::Version,
        memory: Vec<u8>,
    }

    impl BytesHeap {
        fn new(api_version: graph::semver::Version) -> Self {
            Self {
                api_version,
                memory: vec![],
            }
        }
    }

    impl AscHeap for BytesHeap {
        fn raw_new(
            &mut self,
            bytes: &[u8],
            _gas: &GasCounter,
        ) -> Result<u32, DeterministicHostError> {
            self.memory.extend_from_slice(bytes);
            Ok((self.memory.len() - bytes.len()) as u32)
        }

        fn read_u32(&self, offset: u32, gas: &GasCounter) -> Result<u32, DeterministicHostError> {
            let mut data = [std::mem::MaybeUninit::<u8>::uninit(); 4];
            let init = self.read(offset, &mut data, gas)?;
            Ok(u32::from_le_bytes(init.try_into().unwrap()))
        }

        fn read<'a>(
            &self,
            offset: u32,
            buffer: &'a mut [std::mem::MaybeUninit<u8>],
            _gas: &GasCounter,
        ) -> Result<&'a mut [u8], DeterministicHostError> {
            let memory_byte_count = self.memory.len();
            if memory_byte_count == 0 {
                return Err(DeterministicHostError::from(anyhow!(
                    "No memory is allocated"
                )));
            }

            let start_offset = offset as usize;
            let end_offset_exclusive = start_offset + buffer.len();

            if start_offset >= memory_byte_count {
                return Err(DeterministicHostError::from(anyhow!(
                    "Start offset {} is outside of allocated memory, max offset is {}",
                    start_offset,
                    memory_byte_count - 1
                )));
            }

            if end_offset_exclusive > memory_byte_count {
                return Err(DeterministicHostError::from(anyhow!(
                    "End of offset {} is outside of allocated memory, max offset is {}",
                    end_offset_exclusive,
                    memory_byte_count - 1
                )));
            }

            let src = &self.memory[start_offset..end_offset_exclusive];

            Ok(init_slice(src, buffer))
        }

        fn api_version(&self) -> graph::semver::Version {
            self.api_version.clone()
        }

        fn asc_type_id(
            &mut self,
            type_id_index: graph::runtime::IndexForAscTypeId,
        ) -> Result<u32, HostExportError> {
            // Not totally clear what is the purpose of this method, why not a default implementation here?
            Ok(type_id_index as u32)
        }
    }
}
