#![feature(prelude_import)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
pub(crate) mod abi {
    use ethers_contract::abigen;
    pub mod fuel {
        pub use fuel::*;
        #[allow(clippy::too_many_arguments, non_camel_case_types)]
        pub mod fuel {
            #![allow(clippy::enum_variant_names)]
            #![allow(dead_code)]
            #![allow(clippy::type_complexity)]
            #![allow(unused_imports)]
            ///Fuel was auto-generated with ethers-rs Abigen. More information at: https://github.com/gakonst/ethers-rs
            use std::sync::Arc;
            use ethers_core::{
                abi::{Abi, Token, Detokenize, InvalidOutputType, Tokenizable},
                types::*,
            };
            use ethers_contract::{
                Contract, builders::{ContractCall, Event},
                Lazy,
            };
            use ethers_providers::Middleware;
            pub static FUEL_ABI: ethers_contract::Lazy<ethers_core::abi::Abi> = ethers_contract::Lazy::new(||
            {
                ethers_core::utils::__serde_json::from_str(
                        "[\n  {\n    \"inputs\": [\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"bond\",\n        \"type\": \"uint256\"\n      },\n      {\n        \"internalType\": \"uint32\",\n        \"name\": \"finalizationDelay\",\n        \"type\": \"uint32\"\n      },\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"maxClockTime\",\n        \"type\": \"uint256\"\n      },\n      {\n        \"internalType\": \"address\",\n        \"name\": \"leaderSelection\",\n        \"type\": \"address\"\n      },\n      {\n        \"internalType\": \"bytes32\",\n        \"name\": \"genesisValSet\",\n        \"type\": \"bytes32\"\n      },\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"genesisRequiredStake\",\n        \"type\": \"uint256\"\n      }\n    ],\n    \"stateMutability\": \"nonpayable\",\n    \"type\": \"constructor\"\n  },\n  {\n    \"anonymous\": false,\n    \"inputs\": [\n      {\n        \"indexed\": true,\n        \"internalType\": \"bytes32\",\n        \"name\": \"blockRoot\",\n        \"type\": \"bytes32\"\n      },\n      {\n        \"indexed\": true,\n        \"internalType\": \"uint32\",\n        \"name\": \"height\",\n        \"type\": \"uint32\"\n      }\n    ],\n    \"name\": \"BlockCommitted\",\n    \"type\": \"event\"\n  },\n  {\n    \"inputs\": [],\n    \"name\": \"BOND_SIZE\",\n    \"outputs\": [\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"\",\n        \"type\": \"uint256\"\n      }\n    ],\n    \"stateMutability\": \"view\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [],\n    \"name\": \"FINALIZATION_DELAY\",\n    \"outputs\": [\n      {\n        \"internalType\": \"uint32\",\n        \"name\": \"\",\n        \"type\": \"uint32\"\n      }\n    ],\n    \"stateMutability\": \"view\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [],\n    \"name\": \"LEADER_SELECTION\",\n    \"outputs\": [\n      {\n        \"internalType\": \"address\",\n        \"name\": \"\",\n        \"type\": \"address\"\n      }\n    ],\n    \"stateMutability\": \"view\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [],\n    \"name\": \"MAX_BLOCK_DIGESTS\",\n    \"outputs\": [\n      {\n        \"internalType\": \"uint32\",\n        \"name\": \"\",\n        \"type\": \"uint32\"\n      }\n    ],\n    \"stateMutability\": \"view\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [],\n    \"name\": \"MAX_CLOCK_TIME\",\n    \"outputs\": [\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"\",\n        \"type\": \"uint256\"\n      }\n    ],\n    \"stateMutability\": \"view\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [],\n    \"name\": \"MAX_COMPRESSED_TX_BYTES\",\n    \"outputs\": [\n      {\n        \"internalType\": \"uint32\",\n        \"name\": \"\",\n        \"type\": \"uint32\"\n      }\n    ],\n    \"stateMutability\": \"view\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [\n      {\n        \"internalType\": \"uint32\",\n        \"name\": \"minimumBlockNumber\",\n        \"type\": \"uint32\"\n      },\n      {\n        \"internalType\": \"bytes32\",\n        \"name\": \"expectedBlockHash\",\n        \"type\": \"bytes32\"\n      },\n      {\n        \"components\": [\n          {\n            \"internalType\": \"address\",\n            \"name\": \"producer\",\n            \"type\": \"address\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"previousBlockRoot\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint32\",\n            \"name\": \"height\",\n            \"type\": \"uint32\"\n          },\n          {\n            \"internalType\": \"uint32\",\n            \"name\": \"blockNumber\",\n            \"type\": \"uint32\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"digestRoot\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"digestHash\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint16\",\n            \"name\": \"digestLength\",\n            \"type\": \"uint16\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"transactionRoot\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint256\",\n            \"name\": \"transactionSum\",\n            \"type\": \"uint256\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"transactionHash\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint32\",\n            \"name\": \"numTransactions\",\n            \"type\": \"uint32\"\n          },\n          {\n            \"internalType\": \"uint32\",\n            \"name\": \"transactionsDataLength\",\n            \"type\": \"uint32\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"validatorSetHash\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint256\",\n            \"name\": \"requiredStake\",\n            \"type\": \"uint256\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"withdrawalsRoot\",\n            \"type\": \"bytes32\"\n          }\n        ],\n        \"internalType\": \"struct BlockHeader\",\n        \"name\": \"newBlockHeader\",\n        \"type\": \"tuple\"\n      },\n      {\n        \"components\": [\n          {\n            \"internalType\": \"address\",\n            \"name\": \"producer\",\n            \"type\": \"address\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"previousBlockRoot\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint32\",\n            \"name\": \"height\",\n            \"type\": \"uint32\"\n          },\n          {\n            \"internalType\": \"uint32\",\n            \"name\": \"blockNumber\",\n            \"type\": \"uint32\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"digestRoot\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"digestHash\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint16\",\n            \"name\": \"digestLength\",\n            \"type\": \"uint16\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"transactionRoot\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint256\",\n            \"name\": \"transactionSum\",\n            \"type\": \"uint256\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"transactionHash\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint32\",\n            \"name\": \"numTransactions\",\n            \"type\": \"uint32\"\n          },\n          {\n            \"internalType\": \"uint32\",\n            \"name\": \"transactionsDataLength\",\n            \"type\": \"uint32\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"validatorSetHash\",\n            \"type\": \"bytes32\"\n          },\n          {\n            \"internalType\": \"uint256\",\n            \"name\": \"requiredStake\",\n            \"type\": \"uint256\"\n          },\n          {\n            \"internalType\": \"bytes32\",\n            \"name\": \"withdrawalsRoot\",\n            \"type\": \"bytes32\"\n          }\n        ],\n        \"internalType\": \"struct BlockHeader\",\n        \"name\": \"previousBlockHeader\",\n        \"type\": \"tuple\"\n      },\n      {\n        \"internalType\": \"address[]\",\n        \"name\": \"validators\",\n        \"type\": \"address[]\"\n      },\n      {\n        \"internalType\": \"uint256[]\",\n        \"name\": \"stakes\",\n        \"type\": \"uint256[]\"\n      },\n      {\n        \"internalType\": \"bytes[]\",\n        \"name\": \"signatures\",\n        \"type\": \"bytes[]\"\n      },\n      {\n        \"components\": [\n          {\n            \"internalType\": \"address\",\n            \"name\": \"owner\",\n            \"type\": \"address\"\n          },\n          {\n            \"internalType\": \"address\",\n            \"name\": \"token\",\n            \"type\": \"address\"\n          },\n          {\n            \"internalType\": \"uint256\",\n            \"name\": \"amount\",\n            \"type\": \"uint256\"\n          },\n          {\n            \"internalType\": \"uint8\",\n            \"name\": \"precision\",\n            \"type\": \"uint8\"\n          },\n          {\n            \"internalType\": \"uint256\",\n            \"name\": \"nonce\",\n            \"type\": \"uint256\"\n          }\n        ],\n        \"internalType\": \"struct Fuel.Withdrawal[]\",\n        \"name\": \"withdrawals\",\n        \"type\": \"tuple[]\"\n      }\n    ],\n    \"name\": \"commitBlock\",\n    \"outputs\": [],\n    \"stateMutability\": \"nonpayable\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [\n      {\n        \"internalType\": \"address\",\n        \"name\": \"account\",\n        \"type\": \"address\"\n      },\n      {\n        \"internalType\": \"address\",\n        \"name\": \"token\",\n        \"type\": \"address\"\n      },\n      {\n        \"internalType\": \"uint8\",\n        \"name\": \"precisionFactor\",\n        \"type\": \"uint8\"\n      },\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"amount\",\n        \"type\": \"uint256\"\n      }\n    ],\n    \"name\": \"deposit\",\n    \"outputs\": [],\n    \"stateMutability\": \"nonpayable\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [],\n    \"name\": \"s_currentBlockID\",\n    \"outputs\": [\n      {\n        \"internalType\": \"bytes32\",\n        \"name\": \"\",\n        \"type\": \"bytes32\"\n      }\n    ],\n    \"stateMutability\": \"view\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [\n      {\n        \"internalType\": \"address\",\n        \"name\": \"\",\n        \"type\": \"address\"\n      },\n      {\n        \"internalType\": \"address\",\n        \"name\": \"\",\n        \"type\": \"address\"\n      }\n    ],\n    \"name\": \"s_withdrawals\",\n    \"outputs\": [\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"\",\n        \"type\": \"uint256\"\n      }\n    ],\n    \"stateMutability\": \"view\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [\n      {\n        \"internalType\": \"address\",\n        \"name\": \"account\",\n        \"type\": \"address\"\n      },\n      {\n        \"internalType\": \"address\",\n        \"name\": \"token\",\n        \"type\": \"address\"\n      },\n      {\n        \"internalType\": \"uint8\",\n        \"name\": \"precisionFactor\",\n        \"type\": \"uint8\"\n      },\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"amount\",\n        \"type\": \"uint256\"\n      }\n    ],\n    \"name\": \"unsafeDeposit\",\n    \"outputs\": [],\n    \"stateMutability\": \"nonpayable\",\n    \"type\": \"function\"\n  },\n  {\n    \"inputs\": [\n      {\n        \"internalType\": \"address\",\n        \"name\": \"account\",\n        \"type\": \"address\"\n      },\n      {\n        \"internalType\": \"address\",\n        \"name\": \"token\",\n        \"type\": \"address\"\n      },\n      {\n        \"internalType\": \"uint256\",\n        \"name\": \"amount\",\n        \"type\": \"uint256\"\n      }\n    ],\n    \"name\": \"withdraw\",\n    \"outputs\": [],\n    \"stateMutability\": \"nonpayable\",\n    \"type\": \"function\"\n  }\n]",
                    )
                    .expect("invalid abi")
            });
            pub struct Fuel<M>(ethers_contract::Contract<M>);
            impl<M> Clone for Fuel<M> {
                fn clone(&self) -> Self {
                    Fuel(self.0.clone())
                }
            }
            impl<M> std::ops::Deref for Fuel<M> {
                type Target = ethers_contract::Contract<M>;
                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }
            impl<M: ethers_providers::Middleware> std::fmt::Debug for Fuel<M> {
                fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.debug_tuple("Fuel").field(&self.address()).finish()
                }
            }
            impl<M: ethers_providers::Middleware> Fuel<M> {
                /// Creates a new contract instance with the specified `ethers`
                /// client at the given `Address`. The contract derefs to a `ethers::Contract`
                /// object
                pub fn new<T: Into<ethers_core::types::Address>>(
                    address: T,
                    client: ::std::sync::Arc<M>,
                ) -> Self {
                    ethers_contract::Contract::new(
                            address.into(),
                            FUEL_ABI.clone(),
                            client,
                        )
                        .into()
                }
                ///Calls the contract's `BOND_SIZE` (0x23eda127) function
                pub fn bond_size(
                    &self,
                ) -> ethers_contract::builders::ContractCall<
                    M,
                    ethers_core::types::U256,
                > {
                    self.0
                        .method_hash([35, 237, 161, 39], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `FINALIZATION_DELAY` (0x88dd56ec) function
                pub fn finalization_delay(
                    &self,
                ) -> ethers_contract::builders::ContractCall<M, u32> {
                    self.0
                        .method_hash([136, 221, 86, 236], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `LEADER_SELECTION` (0x38403ad4) function
                pub fn leader_selection(
                    &self,
                ) -> ethers_contract::builders::ContractCall<
                    M,
                    ethers_core::types::Address,
                > {
                    self.0
                        .method_hash([56, 64, 58, 212], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `MAX_BLOCK_DIGESTS` (0x3b13825a) function
                pub fn max_block_digests(
                    &self,
                ) -> ethers_contract::builders::ContractCall<M, u32> {
                    self.0
                        .method_hash([59, 19, 130, 90], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `MAX_CLOCK_TIME` (0x4ecfdb20) function
                pub fn max_clock_time(
                    &self,
                ) -> ethers_contract::builders::ContractCall<
                    M,
                    ethers_core::types::U256,
                > {
                    self.0
                        .method_hash([78, 207, 219, 32], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `MAX_COMPRESSED_TX_BYTES` (0x6bf7227e) function
                pub fn max_compressed_tx_bytes(
                    &self,
                ) -> ethers_contract::builders::ContractCall<M, u32> {
                    self.0
                        .method_hash([107, 247, 34, 126], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `commitBlock` (0x38657d27) function
                pub fn commit_block(
                    &self,
                    minimum_block_number: u32,
                    expected_block_hash: [u8; 32],
                    new_block_header: BlockHeader,
                    previous_block_header: BlockHeader,
                    validators: ::std::vec::Vec<ethers_core::types::Address>,
                    stakes: ::std::vec::Vec<ethers_core::types::U256>,
                    signatures: ::std::vec::Vec<ethers_core::types::Bytes>,
                    withdrawals: ::std::vec::Vec<Withdrawal>,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash(
                            [56, 101, 125, 39],
                            (
                                minimum_block_number,
                                expected_block_hash,
                                new_block_header,
                                previous_block_header,
                                validators,
                                stakes,
                                signatures,
                                withdrawals,
                            ),
                        )
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `deposit` (0xec97628d) function
                pub fn deposit(
                    &self,
                    account: ethers_core::types::Address,
                    token: ethers_core::types::Address,
                    precision_factor: u8,
                    amount: ethers_core::types::U256,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash(
                            [236, 151, 98, 141],
                            (account, token, precision_factor, amount),
                        )
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `s_currentBlockID` (0x02f40a2d) function
                pub fn s_current_block_id(
                    &self,
                ) -> ethers_contract::builders::ContractCall<M, [u8; 32]> {
                    self.0
                        .method_hash([2, 244, 10, 45], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `s_withdrawals` (0xcbeb276b) function
                pub fn s_withdrawals(
                    &self,
                    p0: ethers_core::types::Address,
                    p1: ethers_core::types::Address,
                ) -> ethers_contract::builders::ContractCall<
                    M,
                    ethers_core::types::U256,
                > {
                    self.0
                        .method_hash([203, 235, 39, 107], (p0, p1))
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `unsafeDeposit` (0xebee14fc) function
                pub fn unsafe_deposit(
                    &self,
                    account: ethers_core::types::Address,
                    token: ethers_core::types::Address,
                    precision_factor: u8,
                    amount: ethers_core::types::U256,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash(
                            [235, 238, 20, 252],
                            (account, token, precision_factor, amount),
                        )
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `withdraw` (0xd9caed12) function
                pub fn withdraw(
                    &self,
                    account: ethers_core::types::Address,
                    token: ethers_core::types::Address,
                    amount: ethers_core::types::U256,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash([217, 202, 237, 18], (account, token, amount))
                        .expect("method not found (this should never happen)")
                }
                ///Gets the contract's `BlockCommitted` event
                pub fn block_committed_filter(
                    &self,
                ) -> ethers_contract::builders::Event<M, BlockCommittedFilter> {
                    self.0.event()
                }
                /// Returns an [`Event`](#ethers_contract::builders::Event) builder for all events of this contract
                pub fn events(
                    &self,
                ) -> ethers_contract::builders::Event<M, BlockCommittedFilter> {
                    self.0.event_with_filter(Default::default())
                }
            }
            impl<M: ethers_providers::Middleware> From<ethers_contract::Contract<M>>
            for Fuel<M> {
                fn from(contract: ethers_contract::Contract<M>) -> Self {
                    Self(contract)
                }
            }
            #[ethevent(name = "BlockCommitted", abi = "BlockCommitted(bytes32,uint32)")]
            pub struct BlockCommittedFilter {
                #[ethevent(indexed)]
                pub block_root: [u8; 32],
                #[ethevent(indexed)]
                pub height: u32,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for BlockCommittedFilter {
                #[inline]
                fn clone(&self) -> BlockCommittedFilter {
                    BlockCommittedFilter {
                        block_root: ::core::clone::Clone::clone(&self.block_root),
                        height: ::core::clone::Clone::clone(&self.height),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for BlockCommittedFilter {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "BlockCommittedFilter",
                        "block_root",
                        &&self.block_root,
                        "height",
                        &&self.height,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for BlockCommittedFilter {
                #[inline]
                fn default() -> BlockCommittedFilter {
                    BlockCommittedFilter {
                        block_root: ::core::default::Default::default(),
                        height: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for BlockCommittedFilter {}
            #[automatically_derived]
            impl ::core::cmp::Eq for BlockCommittedFilter {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<u32>;
                }
            }
            impl ::core::marker::StructuralPartialEq for BlockCommittedFilter {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for BlockCommittedFilter {
                #[inline]
                fn eq(&self, other: &BlockCommittedFilter) -> bool {
                    self.block_root == other.block_root && self.height == other.height
                }
            }
            impl ethers_core::abi::AbiType for BlockCommittedFilter {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for BlockCommittedFilter {}
            impl ethers_core::abi::Tokenizable for BlockCommittedFilter
            where
                [u8; 32]: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            block_root: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            height: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.block_root.into_token(),
                                self.height.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for BlockCommittedFilter
            where
                [u8; 32]: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthEvent for BlockCommittedFilter {
                fn name() -> ::std::borrow::Cow<'static, str> {
                    "BlockCommitted".into()
                }
                fn signature() -> ethers_core::types::H256 {
                    ethers_core::types::H256([
                        172,
                        216,
                        140,
                        61,
                        113,
                        129,
                        69,
                        70,
                        54,
                        52,
                        114,
                        7,
                        218,
                        115,
                        27,
                        117,
                        123,
                        128,
                        178,
                        105,
                        107,
                        38,
                        216,
                        225,
                        179,
                        120,
                        210,
                        171,
                        94,
                        211,
                        232,
                        114,
                    ])
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "BlockCommitted(bytes32,uint32)".into()
                }
                fn decode_log(
                    log: &ethers_core::abi::RawLog,
                ) -> Result<Self, ethers_core::abi::Error>
                where
                    Self: Sized,
                {
                    let ethers_core::abi::RawLog { data, topics } = log;
                    let event_signature = topics
                        .get(0)
                        .ok_or(ethers_core::abi::Error::InvalidData)?;
                    if event_signature != &Self::signature() {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let topic_types = <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            ethers_core::abi::ParamType::FixedBytes(32usize),
                            ethers_core::abi::ParamType::Uint(32usize),
                        ]),
                    );
                    let data_types = [];
                    let flat_topics = topics
                        .iter()
                        .skip(1)
                        .flat_map(|t| t.as_ref().to_vec())
                        .collect::<Vec<u8>>();
                    let topic_tokens = ethers_core::abi::decode(
                        &topic_types,
                        &flat_topics,
                    )?;
                    if topic_tokens.len() != topics.len() - 1 {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let data_tokens = ethers_core::abi::decode(&data_types, data)?;
                    let tokens: Vec<_> = topic_tokens
                        .into_iter()
                        .chain(data_tokens.into_iter())
                        .collect();
                    ethers_core::abi::Tokenizable::from_token(
                            ethers_core::abi::Token::Tuple(tokens),
                        )
                        .map_err(|_| ethers_core::abi::Error::InvalidData)
                }
                fn is_anonymous() -> bool {
                    false
                }
            }
            impl ::std::fmt::Display for BlockCommittedFilter {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.block_root[..]),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.height)],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `BOND_SIZE` function with signature `BOND_SIZE()` and selector `[35, 237, 161, 39]`
            #[ethcall(name = "BOND_SIZE", abi = "BOND_SIZE()")]
            pub struct BondSizeCall;
            #[automatically_derived]
            impl ::core::clone::Clone for BondSizeCall {
                #[inline]
                fn clone(&self) -> BondSizeCall {
                    BondSizeCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for BondSizeCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "BondSizeCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for BondSizeCall {
                #[inline]
                fn default() -> BondSizeCall {
                    BondSizeCall {}
                }
            }
            impl ::core::marker::StructuralEq for BondSizeCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for BondSizeCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for BondSizeCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for BondSizeCall {
                #[inline]
                fn eq(&self, other: &BondSizeCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for BondSizeCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(BondSizeCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for BondSizeCall {}
            impl ethers_contract::EthCall for BondSizeCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "BOND_SIZE".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [35, 237, 161, 39]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "BOND_SIZE()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for BondSizeCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for BondSizeCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for BondSizeCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `FINALIZATION_DELAY` function with signature `FINALIZATION_DELAY()` and selector `[136, 221, 86, 236]`
            #[ethcall(name = "FINALIZATION_DELAY", abi = "FINALIZATION_DELAY()")]
            pub struct FinalizationDelayCall;
            #[automatically_derived]
            impl ::core::clone::Clone for FinalizationDelayCall {
                #[inline]
                fn clone(&self) -> FinalizationDelayCall {
                    FinalizationDelayCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for FinalizationDelayCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "FinalizationDelayCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for FinalizationDelayCall {
                #[inline]
                fn default() -> FinalizationDelayCall {
                    FinalizationDelayCall {}
                }
            }
            impl ::core::marker::StructuralEq for FinalizationDelayCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for FinalizationDelayCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for FinalizationDelayCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for FinalizationDelayCall {
                #[inline]
                fn eq(&self, other: &FinalizationDelayCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for FinalizationDelayCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(FinalizationDelayCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for FinalizationDelayCall {}
            impl ethers_contract::EthCall for FinalizationDelayCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "FINALIZATION_DELAY".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [136, 221, 86, 236]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "FINALIZATION_DELAY()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for FinalizationDelayCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for FinalizationDelayCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for FinalizationDelayCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `LEADER_SELECTION` function with signature `LEADER_SELECTION()` and selector `[56, 64, 58, 212]`
            #[ethcall(name = "LEADER_SELECTION", abi = "LEADER_SELECTION()")]
            pub struct LeaderSelectionCall;
            #[automatically_derived]
            impl ::core::clone::Clone for LeaderSelectionCall {
                #[inline]
                fn clone(&self) -> LeaderSelectionCall {
                    LeaderSelectionCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for LeaderSelectionCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "LeaderSelectionCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for LeaderSelectionCall {
                #[inline]
                fn default() -> LeaderSelectionCall {
                    LeaderSelectionCall {}
                }
            }
            impl ::core::marker::StructuralEq for LeaderSelectionCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for LeaderSelectionCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for LeaderSelectionCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for LeaderSelectionCall {
                #[inline]
                fn eq(&self, other: &LeaderSelectionCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for LeaderSelectionCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(LeaderSelectionCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for LeaderSelectionCall {}
            impl ethers_contract::EthCall for LeaderSelectionCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "LEADER_SELECTION".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [56, 64, 58, 212]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "LEADER_SELECTION()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for LeaderSelectionCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for LeaderSelectionCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for LeaderSelectionCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `MAX_BLOCK_DIGESTS` function with signature `MAX_BLOCK_DIGESTS()` and selector `[59, 19, 130, 90]`
            #[ethcall(name = "MAX_BLOCK_DIGESTS", abi = "MAX_BLOCK_DIGESTS()")]
            pub struct MaxBlockDigestsCall;
            #[automatically_derived]
            impl ::core::clone::Clone for MaxBlockDigestsCall {
                #[inline]
                fn clone(&self) -> MaxBlockDigestsCall {
                    MaxBlockDigestsCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MaxBlockDigestsCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "MaxBlockDigestsCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for MaxBlockDigestsCall {
                #[inline]
                fn default() -> MaxBlockDigestsCall {
                    MaxBlockDigestsCall {}
                }
            }
            impl ::core::marker::StructuralEq for MaxBlockDigestsCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MaxBlockDigestsCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for MaxBlockDigestsCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MaxBlockDigestsCall {
                #[inline]
                fn eq(&self, other: &MaxBlockDigestsCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for MaxBlockDigestsCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(MaxBlockDigestsCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for MaxBlockDigestsCall {}
            impl ethers_contract::EthCall for MaxBlockDigestsCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "MAX_BLOCK_DIGESTS".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [59, 19, 130, 90]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "MAX_BLOCK_DIGESTS()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for MaxBlockDigestsCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for MaxBlockDigestsCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for MaxBlockDigestsCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `MAX_CLOCK_TIME` function with signature `MAX_CLOCK_TIME()` and selector `[78, 207, 219, 32]`
            #[ethcall(name = "MAX_CLOCK_TIME", abi = "MAX_CLOCK_TIME()")]
            pub struct MaxClockTimeCall;
            #[automatically_derived]
            impl ::core::clone::Clone for MaxClockTimeCall {
                #[inline]
                fn clone(&self) -> MaxClockTimeCall {
                    MaxClockTimeCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MaxClockTimeCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "MaxClockTimeCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for MaxClockTimeCall {
                #[inline]
                fn default() -> MaxClockTimeCall {
                    MaxClockTimeCall {}
                }
            }
            impl ::core::marker::StructuralEq for MaxClockTimeCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MaxClockTimeCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for MaxClockTimeCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MaxClockTimeCall {
                #[inline]
                fn eq(&self, other: &MaxClockTimeCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for MaxClockTimeCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(MaxClockTimeCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for MaxClockTimeCall {}
            impl ethers_contract::EthCall for MaxClockTimeCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "MAX_CLOCK_TIME".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [78, 207, 219, 32]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "MAX_CLOCK_TIME()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for MaxClockTimeCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for MaxClockTimeCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for MaxClockTimeCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `MAX_COMPRESSED_TX_BYTES` function with signature `MAX_COMPRESSED_TX_BYTES()` and selector `[107, 247, 34, 126]`
            #[ethcall(
                name = "MAX_COMPRESSED_TX_BYTES",
                abi = "MAX_COMPRESSED_TX_BYTES()"
            )]
            pub struct MaxCompressedTxBytesCall;
            #[automatically_derived]
            impl ::core::clone::Clone for MaxCompressedTxBytesCall {
                #[inline]
                fn clone(&self) -> MaxCompressedTxBytesCall {
                    MaxCompressedTxBytesCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MaxCompressedTxBytesCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "MaxCompressedTxBytesCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for MaxCompressedTxBytesCall {
                #[inline]
                fn default() -> MaxCompressedTxBytesCall {
                    MaxCompressedTxBytesCall {}
                }
            }
            impl ::core::marker::StructuralEq for MaxCompressedTxBytesCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MaxCompressedTxBytesCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for MaxCompressedTxBytesCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MaxCompressedTxBytesCall {
                #[inline]
                fn eq(&self, other: &MaxCompressedTxBytesCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for MaxCompressedTxBytesCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(MaxCompressedTxBytesCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for MaxCompressedTxBytesCall {}
            impl ethers_contract::EthCall for MaxCompressedTxBytesCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "MAX_COMPRESSED_TX_BYTES".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [107, 247, 34, 126]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "MAX_COMPRESSED_TX_BYTES()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for MaxCompressedTxBytesCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for MaxCompressedTxBytesCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for MaxCompressedTxBytesCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `commitBlock` function with signature `commitBlock(uint32,bytes32,(address,bytes32,uint32,uint32,bytes32,bytes32,uint16,bytes32,uint256,bytes32,uint32,uint32,bytes32,uint256,bytes32),(address,bytes32,uint32,uint32,bytes32,bytes32,uint16,bytes32,uint256,bytes32,uint32,uint32,bytes32,uint256,bytes32),address[],uint256[],bytes[],(address,address,uint256,uint8,uint256)[])` and selector `[56, 101, 125, 39]`
            #[ethcall(
                name = "commitBlock",
                abi = "commitBlock(uint32,bytes32,(address,bytes32,uint32,uint32,bytes32,bytes32,uint16,bytes32,uint256,bytes32,uint32,uint32,bytes32,uint256,bytes32),(address,bytes32,uint32,uint32,bytes32,bytes32,uint16,bytes32,uint256,bytes32,uint32,uint32,bytes32,uint256,bytes32),address[],uint256[],bytes[],(address,address,uint256,uint8,uint256)[])"
            )]
            pub struct CommitBlockCall {
                pub minimum_block_number: u32,
                pub expected_block_hash: [u8; 32],
                pub new_block_header: BlockHeader,
                pub previous_block_header: BlockHeader,
                pub validators: ::std::vec::Vec<ethers_core::types::Address>,
                pub stakes: ::std::vec::Vec<ethers_core::types::U256>,
                pub signatures: ::std::vec::Vec<ethers_core::types::Bytes>,
                pub withdrawals: ::std::vec::Vec<Withdrawal>,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for CommitBlockCall {
                #[inline]
                fn clone(&self) -> CommitBlockCall {
                    CommitBlockCall {
                        minimum_block_number: ::core::clone::Clone::clone(
                            &self.minimum_block_number,
                        ),
                        expected_block_hash: ::core::clone::Clone::clone(
                            &self.expected_block_hash,
                        ),
                        new_block_header: ::core::clone::Clone::clone(
                            &self.new_block_header,
                        ),
                        previous_block_header: ::core::clone::Clone::clone(
                            &self.previous_block_header,
                        ),
                        validators: ::core::clone::Clone::clone(&self.validators),
                        stakes: ::core::clone::Clone::clone(&self.stakes),
                        signatures: ::core::clone::Clone::clone(&self.signatures),
                        withdrawals: ::core::clone::Clone::clone(&self.withdrawals),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for CommitBlockCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    let names: &'static _ = &[
                        "minimum_block_number",
                        "expected_block_hash",
                        "new_block_header",
                        "previous_block_header",
                        "validators",
                        "stakes",
                        "signatures",
                        "withdrawals",
                    ];
                    let values: &[&dyn ::core::fmt::Debug] = &[
                        &&self.minimum_block_number,
                        &&self.expected_block_hash,
                        &&self.new_block_header,
                        &&self.previous_block_header,
                        &&self.validators,
                        &&self.stakes,
                        &&self.signatures,
                        &&self.withdrawals,
                    ];
                    ::core::fmt::Formatter::debug_struct_fields_finish(
                        f,
                        "CommitBlockCall",
                        names,
                        values,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for CommitBlockCall {
                #[inline]
                fn default() -> CommitBlockCall {
                    CommitBlockCall {
                        minimum_block_number: ::core::default::Default::default(),
                        expected_block_hash: ::core::default::Default::default(),
                        new_block_header: ::core::default::Default::default(),
                        previous_block_header: ::core::default::Default::default(),
                        validators: ::core::default::Default::default(),
                        stakes: ::core::default::Default::default(),
                        signatures: ::core::default::Default::default(),
                        withdrawals: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for CommitBlockCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for CommitBlockCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<u32>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<BlockHeader>;
                    let _: ::core::cmp::AssertParamIsEq<
                        ::std::vec::Vec<ethers_core::types::Address>,
                    >;
                    let _: ::core::cmp::AssertParamIsEq<
                        ::std::vec::Vec<ethers_core::types::U256>,
                    >;
                    let _: ::core::cmp::AssertParamIsEq<
                        ::std::vec::Vec<ethers_core::types::Bytes>,
                    >;
                    let _: ::core::cmp::AssertParamIsEq<::std::vec::Vec<Withdrawal>>;
                }
            }
            impl ::core::marker::StructuralPartialEq for CommitBlockCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for CommitBlockCall {
                #[inline]
                fn eq(&self, other: &CommitBlockCall) -> bool {
                    self.minimum_block_number == other.minimum_block_number
                        && self.expected_block_hash == other.expected_block_hash
                        && self.new_block_header == other.new_block_header
                        && self.previous_block_header == other.previous_block_header
                        && self.validators == other.validators
                        && self.stakes == other.stakes
                        && self.signatures == other.signatures
                        && self.withdrawals == other.withdrawals
                }
            }
            impl ethers_core::abi::AbiType for CommitBlockCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <BlockHeader as ethers_core::abi::AbiType>::param_type(),
                                <BlockHeader as ethers_core::abi::AbiType>::param_type(),
                                <::std::vec::Vec<
                                    ethers_core::types::Address,
                                > as ethers_core::abi::AbiType>::param_type(),
                                <::std::vec::Vec<
                                    ethers_core::types::U256,
                                > as ethers_core::abi::AbiType>::param_type(),
                                <::std::vec::Vec<
                                    ethers_core::types::Bytes,
                                > as ethers_core::abi::AbiType>::param_type(),
                                <::std::vec::Vec<
                                    Withdrawal,
                                > as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for CommitBlockCall {}
            impl ethers_core::abi::Tokenizable for CommitBlockCall
            where
                u32: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                BlockHeader: ethers_core::abi::Tokenize,
                BlockHeader: ethers_core::abi::Tokenize,
                ::std::vec::Vec<ethers_core::types::Address>: ethers_core::abi::Tokenize,
                ::std::vec::Vec<ethers_core::types::U256>: ethers_core::abi::Tokenize,
                ::std::vec::Vec<ethers_core::types::Bytes>: ethers_core::abi::Tokenize,
                ::std::vec::Vec<Withdrawal>: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 8usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&8usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            minimum_block_number: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            expected_block_hash: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            new_block_header: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            previous_block_header: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            validators: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            stakes: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            signatures: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            withdrawals: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.minimum_block_number.into_token(),
                                self.expected_block_hash.into_token(),
                                self.new_block_header.into_token(),
                                self.previous_block_header.into_token(),
                                self.validators.into_token(),
                                self.stakes.into_token(),
                                self.signatures.into_token(),
                                self.withdrawals.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for CommitBlockCall
            where
                u32: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                BlockHeader: ethers_core::abi::Tokenize,
                BlockHeader: ethers_core::abi::Tokenize,
                ::std::vec::Vec<ethers_core::types::Address>: ethers_core::abi::Tokenize,
                ::std::vec::Vec<ethers_core::types::U256>: ethers_core::abi::Tokenize,
                ::std::vec::Vec<ethers_core::types::Bytes>: ethers_core::abi::Tokenize,
                ::std::vec::Vec<Withdrawal>: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for CommitBlockCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "commitBlock".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [56, 101, 125, 39]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "commitBlock(uint32,bytes32,(address,bytes32,uint32,uint32,bytes32,bytes32,uint16,bytes32,uint256,bytes32,uint32,uint32,bytes32,uint256,bytes32),(address,bytes32,uint32,uint32,bytes32,bytes32,uint16,bytes32,uint256,bytes32,uint32,uint32,bytes32,uint256,bytes32),address[],uint256[],bytes[],(address,address,uint256,uint8,uint256)[])"
                        .into()
                }
            }
            impl ethers_core::abi::AbiDecode for CommitBlockCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::Uint(32usize),
                        ethers_core::abi::ParamType::FixedBytes(32usize),
                        ethers_core::abi::ParamType::Tuple(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    ethers_core::abi::ParamType::Address,
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(32usize),
                                    ethers_core::abi::ParamType::Uint(32usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(16usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(256usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(32usize),
                                    ethers_core::abi::ParamType::Uint(32usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(256usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                ]),
                            ),
                        ),
                        ethers_core::abi::ParamType::Tuple(
                            <[_]>::into_vec(
                                #[rustc_box]
                                ::alloc::boxed::Box::new([
                                    ethers_core::abi::ParamType::Address,
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(32usize),
                                    ethers_core::abi::ParamType::Uint(32usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(16usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(256usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(32usize),
                                    ethers_core::abi::ParamType::Uint(32usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                    ethers_core::abi::ParamType::Uint(256usize),
                                    ethers_core::abi::ParamType::FixedBytes(32usize),
                                ]),
                            ),
                        ),
                        ethers_core::abi::ParamType::Array(
                            Box::new(ethers_core::abi::ParamType::Address),
                        ),
                        ethers_core::abi::ParamType::Array(
                            Box::new(ethers_core::abi::ParamType::Uint(256usize)),
                        ),
                        ethers_core::abi::ParamType::Array(
                            Box::new(ethers_core::abi::ParamType::Bytes),
                        ),
                        ethers_core::abi::ParamType::Array(
                            Box::new(
                                ethers_core::abi::ParamType::Tuple(
                                    <[_]>::into_vec(
                                        #[rustc_box]
                                        ::alloc::boxed::Box::new([
                                            ethers_core::abi::ParamType::Address,
                                            ethers_core::abi::ParamType::Address,
                                            ethers_core::abi::ParamType::Uint(256usize),
                                            ethers_core::abi::ParamType::Uint(8usize),
                                            ethers_core::abi::ParamType::Uint(256usize),
                                        ]),
                                    ),
                                ),
                            ),
                        ),
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for CommitBlockCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for CommitBlockCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[
                                ::core::fmt::ArgumentV1::new_debug(
                                    &self.minimum_block_number,
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(
                                        &self.expected_block_hash[..],
                                    ),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[
                                ::core::fmt::ArgumentV1::new_debug(&&self.new_block_header),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[
                                ::core::fmt::ArgumentV1::new_debug(
                                    &&self.previous_block_header,
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&&self.validators)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&&self.stakes)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&&self.signatures)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&&self.withdrawals)],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `deposit` function with signature `deposit(address,address,uint8,uint256)` and selector `[236, 151, 98, 141]`
            #[ethcall(name = "deposit", abi = "deposit(address,address,uint8,uint256)")]
            pub struct DepositCall {
                pub account: ethers_core::types::Address,
                pub token: ethers_core::types::Address,
                pub precision_factor: u8,
                pub amount: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for DepositCall {
                #[inline]
                fn clone(&self) -> DepositCall {
                    DepositCall {
                        account: ::core::clone::Clone::clone(&self.account),
                        token: ::core::clone::Clone::clone(&self.token),
                        precision_factor: ::core::clone::Clone::clone(
                            &self.precision_factor,
                        ),
                        amount: ::core::clone::Clone::clone(&self.amount),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for DepositCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field4_finish(
                        f,
                        "DepositCall",
                        "account",
                        &&self.account,
                        "token",
                        &&self.token,
                        "precision_factor",
                        &&self.precision_factor,
                        "amount",
                        &&self.amount,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for DepositCall {
                #[inline]
                fn default() -> DepositCall {
                    DepositCall {
                        account: ::core::default::Default::default(),
                        token: ::core::default::Default::default(),
                        precision_factor: ::core::default::Default::default(),
                        amount: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for DepositCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for DepositCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<u8>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for DepositCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for DepositCall {
                #[inline]
                fn eq(&self, other: &DepositCall) -> bool {
                    self.account == other.account && self.token == other.token
                        && self.precision_factor == other.precision_factor
                        && self.amount == other.amount
                }
            }
            impl ethers_core::abi::AbiType for DepositCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <u8 as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for DepositCall {}
            impl ethers_core::abi::Tokenizable for DepositCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                u8: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 4usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&4usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            account: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            token: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            precision_factor: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.account.into_token(),
                                self.token.into_token(),
                                self.precision_factor.into_token(),
                                self.amount.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for DepositCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                u8: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for DepositCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "deposit".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [236, 151, 98, 141]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "deposit(address,address,uint8,uint256)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for DepositCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::Address,
                        ethers_core::abi::ParamType::Address,
                        ethers_core::abi::ParamType::Uint(8usize),
                        ethers_core::abi::ParamType::Uint(256usize),
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for DepositCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for DepositCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.account)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.token)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.precision_factor)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.amount)],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `s_currentBlockID` function with signature `s_currentBlockID()` and selector `[2, 244, 10, 45]`
            #[ethcall(name = "s_currentBlockID", abi = "s_currentBlockID()")]
            pub struct SCurrentBlockIDCall;
            #[automatically_derived]
            impl ::core::clone::Clone for SCurrentBlockIDCall {
                #[inline]
                fn clone(&self) -> SCurrentBlockIDCall {
                    SCurrentBlockIDCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SCurrentBlockIDCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "SCurrentBlockIDCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SCurrentBlockIDCall {
                #[inline]
                fn default() -> SCurrentBlockIDCall {
                    SCurrentBlockIDCall {}
                }
            }
            impl ::core::marker::StructuralEq for SCurrentBlockIDCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SCurrentBlockIDCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for SCurrentBlockIDCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SCurrentBlockIDCall {
                #[inline]
                fn eq(&self, other: &SCurrentBlockIDCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for SCurrentBlockIDCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(SCurrentBlockIDCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for SCurrentBlockIDCall {}
            impl ethers_contract::EthCall for SCurrentBlockIDCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "s_currentBlockID".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [2, 244, 10, 45]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "s_currentBlockID()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for SCurrentBlockIDCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for SCurrentBlockIDCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for SCurrentBlockIDCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `s_withdrawals` function with signature `s_withdrawals(address,address)` and selector `[203, 235, 39, 107]`
            #[ethcall(name = "s_withdrawals", abi = "s_withdrawals(address,address)")]
            pub struct SWithdrawalsCall(
                pub ethers_core::types::Address,
                pub ethers_core::types::Address,
            );
            #[automatically_derived]
            impl ::core::clone::Clone for SWithdrawalsCall {
                #[inline]
                fn clone(&self) -> SWithdrawalsCall {
                    SWithdrawalsCall(
                        ::core::clone::Clone::clone(&self.0),
                        ::core::clone::Clone::clone(&self.1),
                    )
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SWithdrawalsCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field2_finish(
                        f,
                        "SWithdrawalsCall",
                        &&self.0,
                        &&self.1,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SWithdrawalsCall {
                #[inline]
                fn default() -> SWithdrawalsCall {
                    SWithdrawalsCall(
                        ::core::default::Default::default(),
                        ::core::default::Default::default(),
                    )
                }
            }
            impl ::core::marker::StructuralEq for SWithdrawalsCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SWithdrawalsCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SWithdrawalsCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SWithdrawalsCall {
                #[inline]
                fn eq(&self, other: &SWithdrawalsCall) -> bool {
                    self.0 == other.0 && self.1 == other.1
                }
            }
            impl ethers_core::abi::AbiType for SWithdrawalsCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SWithdrawalsCall {}
            impl ethers_core::abi::Tokenizable for SWithdrawalsCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.0.into_token(),
                                self.1.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SWithdrawalsCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for SWithdrawalsCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "s_withdrawals".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [203, 235, 39, 107]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "s_withdrawals(address,address)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for SWithdrawalsCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::Address,
                        ethers_core::abi::ParamType::Address,
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for SWithdrawalsCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for SWithdrawalsCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.0)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.1)],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `unsafeDeposit` function with signature `unsafeDeposit(address,address,uint8,uint256)` and selector `[235, 238, 20, 252]`
            #[ethcall(
                name = "unsafeDeposit",
                abi = "unsafeDeposit(address,address,uint8,uint256)"
            )]
            pub struct UnsafeDepositCall {
                pub account: ethers_core::types::Address,
                pub token: ethers_core::types::Address,
                pub precision_factor: u8,
                pub amount: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for UnsafeDepositCall {
                #[inline]
                fn clone(&self) -> UnsafeDepositCall {
                    UnsafeDepositCall {
                        account: ::core::clone::Clone::clone(&self.account),
                        token: ::core::clone::Clone::clone(&self.token),
                        precision_factor: ::core::clone::Clone::clone(
                            &self.precision_factor,
                        ),
                        amount: ::core::clone::Clone::clone(&self.amount),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for UnsafeDepositCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field4_finish(
                        f,
                        "UnsafeDepositCall",
                        "account",
                        &&self.account,
                        "token",
                        &&self.token,
                        "precision_factor",
                        &&self.precision_factor,
                        "amount",
                        &&self.amount,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for UnsafeDepositCall {
                #[inline]
                fn default() -> UnsafeDepositCall {
                    UnsafeDepositCall {
                        account: ::core::default::Default::default(),
                        token: ::core::default::Default::default(),
                        precision_factor: ::core::default::Default::default(),
                        amount: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for UnsafeDepositCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for UnsafeDepositCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<u8>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for UnsafeDepositCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for UnsafeDepositCall {
                #[inline]
                fn eq(&self, other: &UnsafeDepositCall) -> bool {
                    self.account == other.account && self.token == other.token
                        && self.precision_factor == other.precision_factor
                        && self.amount == other.amount
                }
            }
            impl ethers_core::abi::AbiType for UnsafeDepositCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <u8 as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for UnsafeDepositCall {}
            impl ethers_core::abi::Tokenizable for UnsafeDepositCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                u8: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 4usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&4usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            account: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            token: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            precision_factor: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.account.into_token(),
                                self.token.into_token(),
                                self.precision_factor.into_token(),
                                self.amount.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for UnsafeDepositCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                u8: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for UnsafeDepositCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "unsafeDeposit".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [235, 238, 20, 252]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "unsafeDeposit(address,address,uint8,uint256)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for UnsafeDepositCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::Address,
                        ethers_core::abi::ParamType::Address,
                        ethers_core::abi::ParamType::Uint(8usize),
                        ethers_core::abi::ParamType::Uint(256usize),
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for UnsafeDepositCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for UnsafeDepositCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.account)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.token)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.precision_factor)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.amount)],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `withdraw` function with signature `withdraw(address,address,uint256)` and selector `[217, 202, 237, 18]`
            #[ethcall(name = "withdraw", abi = "withdraw(address,address,uint256)")]
            pub struct WithdrawCall {
                pub account: ethers_core::types::Address,
                pub token: ethers_core::types::Address,
                pub amount: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for WithdrawCall {
                #[inline]
                fn clone(&self) -> WithdrawCall {
                    WithdrawCall {
                        account: ::core::clone::Clone::clone(&self.account),
                        token: ::core::clone::Clone::clone(&self.token),
                        amount: ::core::clone::Clone::clone(&self.amount),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for WithdrawCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field3_finish(
                        f,
                        "WithdrawCall",
                        "account",
                        &&self.account,
                        "token",
                        &&self.token,
                        "amount",
                        &&self.amount,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for WithdrawCall {
                #[inline]
                fn default() -> WithdrawCall {
                    WithdrawCall {
                        account: ::core::default::Default::default(),
                        token: ::core::default::Default::default(),
                        amount: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for WithdrawCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for WithdrawCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for WithdrawCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for WithdrawCall {
                #[inline]
                fn eq(&self, other: &WithdrawCall) -> bool {
                    self.account == other.account && self.token == other.token
                        && self.amount == other.amount
                }
            }
            impl ethers_core::abi::AbiType for WithdrawCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for WithdrawCall {}
            impl ethers_core::abi::Tokenizable for WithdrawCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 3usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&3usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            account: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            token: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.account.into_token(),
                                self.token.into_token(),
                                self.amount.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for WithdrawCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for WithdrawCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "withdraw".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [217, 202, 237, 18]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "withdraw(address,address,uint256)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for WithdrawCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::Address,
                        ethers_core::abi::ParamType::Address,
                        ethers_core::abi::ParamType::Uint(256usize),
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for WithdrawCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for WithdrawCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.account)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.token)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.amount)],
                        ),
                    )?;
                    Ok(())
                }
            }
            pub enum FuelCalls {
                BondSize(BondSizeCall),
                FinalizationDelay(FinalizationDelayCall),
                LeaderSelection(LeaderSelectionCall),
                MaxBlockDigests(MaxBlockDigestsCall),
                MaxClockTime(MaxClockTimeCall),
                MaxCompressedTxBytes(MaxCompressedTxBytesCall),
                CommitBlock(CommitBlockCall),
                Deposit(DepositCall),
                SCurrentBlockID(SCurrentBlockIDCall),
                SWithdrawals(SWithdrawalsCall),
                UnsafeDeposit(UnsafeDepositCall),
                Withdraw(WithdrawCall),
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for FuelCalls {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        FuelCalls::BondSize(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "BondSize",
                                &__self_0,
                            )
                        }
                        FuelCalls::FinalizationDelay(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "FinalizationDelay",
                                &__self_0,
                            )
                        }
                        FuelCalls::LeaderSelection(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "LeaderSelection",
                                &__self_0,
                            )
                        }
                        FuelCalls::MaxBlockDigests(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "MaxBlockDigests",
                                &__self_0,
                            )
                        }
                        FuelCalls::MaxClockTime(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "MaxClockTime",
                                &__self_0,
                            )
                        }
                        FuelCalls::MaxCompressedTxBytes(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "MaxCompressedTxBytes",
                                &__self_0,
                            )
                        }
                        FuelCalls::CommitBlock(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "CommitBlock",
                                &__self_0,
                            )
                        }
                        FuelCalls::Deposit(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Deposit",
                                &__self_0,
                            )
                        }
                        FuelCalls::SCurrentBlockID(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "SCurrentBlockID",
                                &__self_0,
                            )
                        }
                        FuelCalls::SWithdrawals(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "SWithdrawals",
                                &__self_0,
                            )
                        }
                        FuelCalls::UnsafeDeposit(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "UnsafeDeposit",
                                &__self_0,
                            )
                        }
                        FuelCalls::Withdraw(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Withdraw",
                                &__self_0,
                            )
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::clone::Clone for FuelCalls {
                #[inline]
                fn clone(&self) -> FuelCalls {
                    match self {
                        FuelCalls::BondSize(__self_0) => {
                            FuelCalls::BondSize(::core::clone::Clone::clone(__self_0))
                        }
                        FuelCalls::FinalizationDelay(__self_0) => {
                            FuelCalls::FinalizationDelay(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        FuelCalls::LeaderSelection(__self_0) => {
                            FuelCalls::LeaderSelection(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        FuelCalls::MaxBlockDigests(__self_0) => {
                            FuelCalls::MaxBlockDigests(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        FuelCalls::MaxClockTime(__self_0) => {
                            FuelCalls::MaxClockTime(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        FuelCalls::MaxCompressedTxBytes(__self_0) => {
                            FuelCalls::MaxCompressedTxBytes(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        FuelCalls::CommitBlock(__self_0) => {
                            FuelCalls::CommitBlock(::core::clone::Clone::clone(__self_0))
                        }
                        FuelCalls::Deposit(__self_0) => {
                            FuelCalls::Deposit(::core::clone::Clone::clone(__self_0))
                        }
                        FuelCalls::SCurrentBlockID(__self_0) => {
                            FuelCalls::SCurrentBlockID(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        FuelCalls::SWithdrawals(__self_0) => {
                            FuelCalls::SWithdrawals(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        FuelCalls::UnsafeDeposit(__self_0) => {
                            FuelCalls::UnsafeDeposit(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        FuelCalls::Withdraw(__self_0) => {
                            FuelCalls::Withdraw(::core::clone::Clone::clone(__self_0))
                        }
                    }
                }
            }
            impl ::core::marker::StructuralPartialEq for FuelCalls {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for FuelCalls {
                #[inline]
                fn eq(&self, other: &FuelCalls) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                        && match (self, other) {
                            (
                                FuelCalls::BondSize(__self_0),
                                FuelCalls::BondSize(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::FinalizationDelay(__self_0),
                                FuelCalls::FinalizationDelay(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::LeaderSelection(__self_0),
                                FuelCalls::LeaderSelection(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::MaxBlockDigests(__self_0),
                                FuelCalls::MaxBlockDigests(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::MaxClockTime(__self_0),
                                FuelCalls::MaxClockTime(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::MaxCompressedTxBytes(__self_0),
                                FuelCalls::MaxCompressedTxBytes(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::CommitBlock(__self_0),
                                FuelCalls::CommitBlock(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::Deposit(__self_0),
                                FuelCalls::Deposit(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::SCurrentBlockID(__self_0),
                                FuelCalls::SCurrentBlockID(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::SWithdrawals(__self_0),
                                FuelCalls::SWithdrawals(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::UnsafeDeposit(__self_0),
                                FuelCalls::UnsafeDeposit(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                FuelCalls::Withdraw(__self_0),
                                FuelCalls::Withdraw(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
            impl ::core::marker::StructuralEq for FuelCalls {}
            #[automatically_derived]
            impl ::core::cmp::Eq for FuelCalls {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<BondSizeCall>;
                    let _: ::core::cmp::AssertParamIsEq<FinalizationDelayCall>;
                    let _: ::core::cmp::AssertParamIsEq<LeaderSelectionCall>;
                    let _: ::core::cmp::AssertParamIsEq<MaxBlockDigestsCall>;
                    let _: ::core::cmp::AssertParamIsEq<MaxClockTimeCall>;
                    let _: ::core::cmp::AssertParamIsEq<MaxCompressedTxBytesCall>;
                    let _: ::core::cmp::AssertParamIsEq<CommitBlockCall>;
                    let _: ::core::cmp::AssertParamIsEq<DepositCall>;
                    let _: ::core::cmp::AssertParamIsEq<SCurrentBlockIDCall>;
                    let _: ::core::cmp::AssertParamIsEq<SWithdrawalsCall>;
                    let _: ::core::cmp::AssertParamIsEq<UnsafeDepositCall>;
                    let _: ::core::cmp::AssertParamIsEq<WithdrawCall>;
                }
            }
            impl ethers_core::abi::Tokenizable for FuelCalls {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let Ok(decoded) = BondSizeCall::from_token(token.clone()) {
                        return Ok(FuelCalls::BondSize(decoded));
                    }
                    if let Ok(decoded)
                        = FinalizationDelayCall::from_token(token.clone()) {
                        return Ok(FuelCalls::FinalizationDelay(decoded));
                    }
                    if let Ok(decoded) = LeaderSelectionCall::from_token(token.clone()) {
                        return Ok(FuelCalls::LeaderSelection(decoded));
                    }
                    if let Ok(decoded) = MaxBlockDigestsCall::from_token(token.clone()) {
                        return Ok(FuelCalls::MaxBlockDigests(decoded));
                    }
                    if let Ok(decoded) = MaxClockTimeCall::from_token(token.clone()) {
                        return Ok(FuelCalls::MaxClockTime(decoded));
                    }
                    if let Ok(decoded)
                        = MaxCompressedTxBytesCall::from_token(token.clone()) {
                        return Ok(FuelCalls::MaxCompressedTxBytes(decoded));
                    }
                    if let Ok(decoded) = CommitBlockCall::from_token(token.clone()) {
                        return Ok(FuelCalls::CommitBlock(decoded));
                    }
                    if let Ok(decoded) = DepositCall::from_token(token.clone()) {
                        return Ok(FuelCalls::Deposit(decoded));
                    }
                    if let Ok(decoded) = SCurrentBlockIDCall::from_token(token.clone()) {
                        return Ok(FuelCalls::SCurrentBlockID(decoded));
                    }
                    if let Ok(decoded) = SWithdrawalsCall::from_token(token.clone()) {
                        return Ok(FuelCalls::SWithdrawals(decoded));
                    }
                    if let Ok(decoded) = UnsafeDepositCall::from_token(token.clone()) {
                        return Ok(FuelCalls::UnsafeDeposit(decoded));
                    }
                    if let Ok(decoded) = WithdrawCall::from_token(token.clone()) {
                        return Ok(FuelCalls::Withdraw(decoded));
                    }
                    Err(
                        ethers_core::abi::InvalidOutputType(
                            "Failed to decode all type variants".to_string(),
                        ),
                    )
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    match self {
                        FuelCalls::BondSize(element) => element.into_token(),
                        FuelCalls::FinalizationDelay(element) => element.into_token(),
                        FuelCalls::LeaderSelection(element) => element.into_token(),
                        FuelCalls::MaxBlockDigests(element) => element.into_token(),
                        FuelCalls::MaxClockTime(element) => element.into_token(),
                        FuelCalls::MaxCompressedTxBytes(element) => element.into_token(),
                        FuelCalls::CommitBlock(element) => element.into_token(),
                        FuelCalls::Deposit(element) => element.into_token(),
                        FuelCalls::SCurrentBlockID(element) => element.into_token(),
                        FuelCalls::SWithdrawals(element) => element.into_token(),
                        FuelCalls::UnsafeDeposit(element) => element.into_token(),
                        FuelCalls::Withdraw(element) => element.into_token(),
                    }
                }
            }
            impl ethers_core::abi::TokenizableItem for FuelCalls {}
            impl ethers_core::abi::AbiDecode for FuelCalls {
                fn decode(
                    data: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let Ok(decoded)
                        = <BondSizeCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::BondSize(decoded));
                    }
                    if let Ok(decoded)
                        = <FinalizationDelayCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::FinalizationDelay(decoded));
                    }
                    if let Ok(decoded)
                        = <LeaderSelectionCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::LeaderSelection(decoded));
                    }
                    if let Ok(decoded)
                        = <MaxBlockDigestsCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::MaxBlockDigests(decoded));
                    }
                    if let Ok(decoded)
                        = <MaxClockTimeCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::MaxClockTime(decoded));
                    }
                    if let Ok(decoded)
                        = <MaxCompressedTxBytesCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::MaxCompressedTxBytes(decoded));
                    }
                    if let Ok(decoded)
                        = <CommitBlockCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::CommitBlock(decoded));
                    }
                    if let Ok(decoded)
                        = <DepositCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::Deposit(decoded));
                    }
                    if let Ok(decoded)
                        = <SCurrentBlockIDCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::SCurrentBlockID(decoded));
                    }
                    if let Ok(decoded)
                        = <SWithdrawalsCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::SWithdrawals(decoded));
                    }
                    if let Ok(decoded)
                        = <UnsafeDepositCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::UnsafeDeposit(decoded));
                    }
                    if let Ok(decoded)
                        = <WithdrawCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(FuelCalls::Withdraw(decoded));
                    }
                    Err(ethers_core::abi::Error::InvalidData.into())
                }
            }
            impl ethers_core::abi::AbiEncode for FuelCalls {
                fn encode(self) -> Vec<u8> {
                    match self {
                        FuelCalls::BondSize(element) => element.encode(),
                        FuelCalls::FinalizationDelay(element) => element.encode(),
                        FuelCalls::LeaderSelection(element) => element.encode(),
                        FuelCalls::MaxBlockDigests(element) => element.encode(),
                        FuelCalls::MaxClockTime(element) => element.encode(),
                        FuelCalls::MaxCompressedTxBytes(element) => element.encode(),
                        FuelCalls::CommitBlock(element) => element.encode(),
                        FuelCalls::Deposit(element) => element.encode(),
                        FuelCalls::SCurrentBlockID(element) => element.encode(),
                        FuelCalls::SWithdrawals(element) => element.encode(),
                        FuelCalls::UnsafeDeposit(element) => element.encode(),
                        FuelCalls::Withdraw(element) => element.encode(),
                    }
                }
            }
            impl ::std::fmt::Display for FuelCalls {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    match self {
                        FuelCalls::BondSize(element) => element.fmt(f),
                        FuelCalls::FinalizationDelay(element) => element.fmt(f),
                        FuelCalls::LeaderSelection(element) => element.fmt(f),
                        FuelCalls::MaxBlockDigests(element) => element.fmt(f),
                        FuelCalls::MaxClockTime(element) => element.fmt(f),
                        FuelCalls::MaxCompressedTxBytes(element) => element.fmt(f),
                        FuelCalls::CommitBlock(element) => element.fmt(f),
                        FuelCalls::Deposit(element) => element.fmt(f),
                        FuelCalls::SCurrentBlockID(element) => element.fmt(f),
                        FuelCalls::SWithdrawals(element) => element.fmt(f),
                        FuelCalls::UnsafeDeposit(element) => element.fmt(f),
                        FuelCalls::Withdraw(element) => element.fmt(f),
                    }
                }
            }
            impl ::std::convert::From<BondSizeCall> for FuelCalls {
                fn from(var: BondSizeCall) -> Self {
                    FuelCalls::BondSize(var)
                }
            }
            impl ::std::convert::From<FinalizationDelayCall> for FuelCalls {
                fn from(var: FinalizationDelayCall) -> Self {
                    FuelCalls::FinalizationDelay(var)
                }
            }
            impl ::std::convert::From<LeaderSelectionCall> for FuelCalls {
                fn from(var: LeaderSelectionCall) -> Self {
                    FuelCalls::LeaderSelection(var)
                }
            }
            impl ::std::convert::From<MaxBlockDigestsCall> for FuelCalls {
                fn from(var: MaxBlockDigestsCall) -> Self {
                    FuelCalls::MaxBlockDigests(var)
                }
            }
            impl ::std::convert::From<MaxClockTimeCall> for FuelCalls {
                fn from(var: MaxClockTimeCall) -> Self {
                    FuelCalls::MaxClockTime(var)
                }
            }
            impl ::std::convert::From<MaxCompressedTxBytesCall> for FuelCalls {
                fn from(var: MaxCompressedTxBytesCall) -> Self {
                    FuelCalls::MaxCompressedTxBytes(var)
                }
            }
            impl ::std::convert::From<CommitBlockCall> for FuelCalls {
                fn from(var: CommitBlockCall) -> Self {
                    FuelCalls::CommitBlock(var)
                }
            }
            impl ::std::convert::From<DepositCall> for FuelCalls {
                fn from(var: DepositCall) -> Self {
                    FuelCalls::Deposit(var)
                }
            }
            impl ::std::convert::From<SCurrentBlockIDCall> for FuelCalls {
                fn from(var: SCurrentBlockIDCall) -> Self {
                    FuelCalls::SCurrentBlockID(var)
                }
            }
            impl ::std::convert::From<SWithdrawalsCall> for FuelCalls {
                fn from(var: SWithdrawalsCall) -> Self {
                    FuelCalls::SWithdrawals(var)
                }
            }
            impl ::std::convert::From<UnsafeDepositCall> for FuelCalls {
                fn from(var: UnsafeDepositCall) -> Self {
                    FuelCalls::UnsafeDeposit(var)
                }
            }
            impl ::std::convert::From<WithdrawCall> for FuelCalls {
                fn from(var: WithdrawCall) -> Self {
                    FuelCalls::Withdraw(var)
                }
            }
            ///Container type for all return fields from the `BOND_SIZE` function with signature `BOND_SIZE()` and selector `[35, 237, 161, 39]`
            pub struct BondSizeReturn(pub ethers_core::types::U256);
            #[automatically_derived]
            impl ::core::clone::Clone for BondSizeReturn {
                #[inline]
                fn clone(&self) -> BondSizeReturn {
                    BondSizeReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for BondSizeReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "BondSizeReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for BondSizeReturn {
                #[inline]
                fn default() -> BondSizeReturn {
                    BondSizeReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for BondSizeReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for BondSizeReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for BondSizeReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for BondSizeReturn {
                #[inline]
                fn eq(&self, other: &BondSizeReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for BondSizeReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for BondSizeReturn {}
            impl ethers_core::abi::Tokenizable for BondSizeReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for BondSizeReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for BondSizeReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for BondSizeReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `FINALIZATION_DELAY` function with signature `FINALIZATION_DELAY()` and selector `[136, 221, 86, 236]`
            pub struct FinalizationDelayReturn(pub u32);
            #[automatically_derived]
            impl ::core::clone::Clone for FinalizationDelayReturn {
                #[inline]
                fn clone(&self) -> FinalizationDelayReturn {
                    FinalizationDelayReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for FinalizationDelayReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "FinalizationDelayReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for FinalizationDelayReturn {
                #[inline]
                fn default() -> FinalizationDelayReturn {
                    FinalizationDelayReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for FinalizationDelayReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for FinalizationDelayReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<u32>;
                }
            }
            impl ::core::marker::StructuralPartialEq for FinalizationDelayReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for FinalizationDelayReturn {
                #[inline]
                fn eq(&self, other: &FinalizationDelayReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for FinalizationDelayReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for FinalizationDelayReturn {}
            impl ethers_core::abi::Tokenizable for FinalizationDelayReturn
            where
                u32: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for FinalizationDelayReturn
            where
                u32: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for FinalizationDelayReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for FinalizationDelayReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `LEADER_SELECTION` function with signature `LEADER_SELECTION()` and selector `[56, 64, 58, 212]`
            pub struct LeaderSelectionReturn(pub ethers_core::types::Address);
            #[automatically_derived]
            impl ::core::clone::Clone for LeaderSelectionReturn {
                #[inline]
                fn clone(&self) -> LeaderSelectionReturn {
                    LeaderSelectionReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for LeaderSelectionReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "LeaderSelectionReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for LeaderSelectionReturn {
                #[inline]
                fn default() -> LeaderSelectionReturn {
                    LeaderSelectionReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for LeaderSelectionReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for LeaderSelectionReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                }
            }
            impl ::core::marker::StructuralPartialEq for LeaderSelectionReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for LeaderSelectionReturn {
                #[inline]
                fn eq(&self, other: &LeaderSelectionReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for LeaderSelectionReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for LeaderSelectionReturn {}
            impl ethers_core::abi::Tokenizable for LeaderSelectionReturn
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for LeaderSelectionReturn
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for LeaderSelectionReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for LeaderSelectionReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `MAX_BLOCK_DIGESTS` function with signature `MAX_BLOCK_DIGESTS()` and selector `[59, 19, 130, 90]`
            pub struct MaxBlockDigestsReturn(pub u32);
            #[automatically_derived]
            impl ::core::clone::Clone for MaxBlockDigestsReturn {
                #[inline]
                fn clone(&self) -> MaxBlockDigestsReturn {
                    MaxBlockDigestsReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MaxBlockDigestsReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "MaxBlockDigestsReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for MaxBlockDigestsReturn {
                #[inline]
                fn default() -> MaxBlockDigestsReturn {
                    MaxBlockDigestsReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for MaxBlockDigestsReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MaxBlockDigestsReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<u32>;
                }
            }
            impl ::core::marker::StructuralPartialEq for MaxBlockDigestsReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MaxBlockDigestsReturn {
                #[inline]
                fn eq(&self, other: &MaxBlockDigestsReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for MaxBlockDigestsReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for MaxBlockDigestsReturn {}
            impl ethers_core::abi::Tokenizable for MaxBlockDigestsReturn
            where
                u32: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for MaxBlockDigestsReturn
            where
                u32: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for MaxBlockDigestsReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for MaxBlockDigestsReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `MAX_CLOCK_TIME` function with signature `MAX_CLOCK_TIME()` and selector `[78, 207, 219, 32]`
            pub struct MaxClockTimeReturn(pub ethers_core::types::U256);
            #[automatically_derived]
            impl ::core::clone::Clone for MaxClockTimeReturn {
                #[inline]
                fn clone(&self) -> MaxClockTimeReturn {
                    MaxClockTimeReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MaxClockTimeReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "MaxClockTimeReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for MaxClockTimeReturn {
                #[inline]
                fn default() -> MaxClockTimeReturn {
                    MaxClockTimeReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for MaxClockTimeReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MaxClockTimeReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for MaxClockTimeReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MaxClockTimeReturn {
                #[inline]
                fn eq(&self, other: &MaxClockTimeReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for MaxClockTimeReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for MaxClockTimeReturn {}
            impl ethers_core::abi::Tokenizable for MaxClockTimeReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for MaxClockTimeReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for MaxClockTimeReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for MaxClockTimeReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `MAX_COMPRESSED_TX_BYTES` function with signature `MAX_COMPRESSED_TX_BYTES()` and selector `[107, 247, 34, 126]`
            pub struct MaxCompressedTxBytesReturn(pub u32);
            #[automatically_derived]
            impl ::core::clone::Clone for MaxCompressedTxBytesReturn {
                #[inline]
                fn clone(&self) -> MaxCompressedTxBytesReturn {
                    MaxCompressedTxBytesReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MaxCompressedTxBytesReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "MaxCompressedTxBytesReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for MaxCompressedTxBytesReturn {
                #[inline]
                fn default() -> MaxCompressedTxBytesReturn {
                    MaxCompressedTxBytesReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for MaxCompressedTxBytesReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MaxCompressedTxBytesReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<u32>;
                }
            }
            impl ::core::marker::StructuralPartialEq for MaxCompressedTxBytesReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MaxCompressedTxBytesReturn {
                #[inline]
                fn eq(&self, other: &MaxCompressedTxBytesReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for MaxCompressedTxBytesReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for MaxCompressedTxBytesReturn {}
            impl ethers_core::abi::Tokenizable for MaxCompressedTxBytesReturn
            where
                u32: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for MaxCompressedTxBytesReturn
            where
                u32: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for MaxCompressedTxBytesReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for MaxCompressedTxBytesReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `s_currentBlockID` function with signature `s_currentBlockID()` and selector `[2, 244, 10, 45]`
            pub struct SCurrentBlockIDReturn(pub [u8; 32]);
            #[automatically_derived]
            impl ::core::clone::Clone for SCurrentBlockIDReturn {
                #[inline]
                fn clone(&self) -> SCurrentBlockIDReturn {
                    SCurrentBlockIDReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SCurrentBlockIDReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "SCurrentBlockIDReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SCurrentBlockIDReturn {
                #[inline]
                fn default() -> SCurrentBlockIDReturn {
                    SCurrentBlockIDReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for SCurrentBlockIDReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SCurrentBlockIDReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SCurrentBlockIDReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SCurrentBlockIDReturn {
                #[inline]
                fn eq(&self, other: &SCurrentBlockIDReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for SCurrentBlockIDReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SCurrentBlockIDReturn {}
            impl ethers_core::abi::Tokenizable for SCurrentBlockIDReturn
            where
                [u8; 32]: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SCurrentBlockIDReturn
            where
                [u8; 32]: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for SCurrentBlockIDReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for SCurrentBlockIDReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `s_withdrawals` function with signature `s_withdrawals(address,address)` and selector `[203, 235, 39, 107]`
            pub struct SWithdrawalsReturn(pub ethers_core::types::U256);
            #[automatically_derived]
            impl ::core::clone::Clone for SWithdrawalsReturn {
                #[inline]
                fn clone(&self) -> SWithdrawalsReturn {
                    SWithdrawalsReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SWithdrawalsReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "SWithdrawalsReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SWithdrawalsReturn {
                #[inline]
                fn default() -> SWithdrawalsReturn {
                    SWithdrawalsReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for SWithdrawalsReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SWithdrawalsReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SWithdrawalsReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SWithdrawalsReturn {
                #[inline]
                fn eq(&self, other: &SWithdrawalsReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for SWithdrawalsReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SWithdrawalsReturn {}
            impl ethers_core::abi::Tokenizable for SWithdrawalsReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SWithdrawalsReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for SWithdrawalsReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for SWithdrawalsReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///`BlockHeader(address,bytes32,uint32,uint32,bytes32,bytes32,uint16,bytes32,uint256,bytes32,uint32,uint32,bytes32,uint256,bytes32)`
            pub struct BlockHeader {
                pub producer: ethers_core::types::Address,
                pub previous_block_root: [u8; 32],
                pub height: u32,
                pub block_number: u32,
                pub digest_root: [u8; 32],
                pub digest_hash: [u8; 32],
                pub digest_length: u16,
                pub transaction_root: [u8; 32],
                pub transaction_sum: ethers_core::types::U256,
                pub transaction_hash: [u8; 32],
                pub num_transactions: u32,
                pub transactions_data_length: u32,
                pub validator_set_hash: [u8; 32],
                pub required_stake: ethers_core::types::U256,
                pub withdrawals_root: [u8; 32],
            }
            #[automatically_derived]
            impl ::core::clone::Clone for BlockHeader {
                #[inline]
                fn clone(&self) -> BlockHeader {
                    BlockHeader {
                        producer: ::core::clone::Clone::clone(&self.producer),
                        previous_block_root: ::core::clone::Clone::clone(
                            &self.previous_block_root,
                        ),
                        height: ::core::clone::Clone::clone(&self.height),
                        block_number: ::core::clone::Clone::clone(&self.block_number),
                        digest_root: ::core::clone::Clone::clone(&self.digest_root),
                        digest_hash: ::core::clone::Clone::clone(&self.digest_hash),
                        digest_length: ::core::clone::Clone::clone(&self.digest_length),
                        transaction_root: ::core::clone::Clone::clone(
                            &self.transaction_root,
                        ),
                        transaction_sum: ::core::clone::Clone::clone(
                            &self.transaction_sum,
                        ),
                        transaction_hash: ::core::clone::Clone::clone(
                            &self.transaction_hash,
                        ),
                        num_transactions: ::core::clone::Clone::clone(
                            &self.num_transactions,
                        ),
                        transactions_data_length: ::core::clone::Clone::clone(
                            &self.transactions_data_length,
                        ),
                        validator_set_hash: ::core::clone::Clone::clone(
                            &self.validator_set_hash,
                        ),
                        required_stake: ::core::clone::Clone::clone(
                            &self.required_stake,
                        ),
                        withdrawals_root: ::core::clone::Clone::clone(
                            &self.withdrawals_root,
                        ),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for BlockHeader {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    let names: &'static _ = &[
                        "producer",
                        "previous_block_root",
                        "height",
                        "block_number",
                        "digest_root",
                        "digest_hash",
                        "digest_length",
                        "transaction_root",
                        "transaction_sum",
                        "transaction_hash",
                        "num_transactions",
                        "transactions_data_length",
                        "validator_set_hash",
                        "required_stake",
                        "withdrawals_root",
                    ];
                    let values: &[&dyn ::core::fmt::Debug] = &[
                        &&self.producer,
                        &&self.previous_block_root,
                        &&self.height,
                        &&self.block_number,
                        &&self.digest_root,
                        &&self.digest_hash,
                        &&self.digest_length,
                        &&self.transaction_root,
                        &&self.transaction_sum,
                        &&self.transaction_hash,
                        &&self.num_transactions,
                        &&self.transactions_data_length,
                        &&self.validator_set_hash,
                        &&self.required_stake,
                        &&self.withdrawals_root,
                    ];
                    ::core::fmt::Formatter::debug_struct_fields_finish(
                        f,
                        "BlockHeader",
                        names,
                        values,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for BlockHeader {
                #[inline]
                fn default() -> BlockHeader {
                    BlockHeader {
                        producer: ::core::default::Default::default(),
                        previous_block_root: ::core::default::Default::default(),
                        height: ::core::default::Default::default(),
                        block_number: ::core::default::Default::default(),
                        digest_root: ::core::default::Default::default(),
                        digest_hash: ::core::default::Default::default(),
                        digest_length: ::core::default::Default::default(),
                        transaction_root: ::core::default::Default::default(),
                        transaction_sum: ::core::default::Default::default(),
                        transaction_hash: ::core::default::Default::default(),
                        num_transactions: ::core::default::Default::default(),
                        transactions_data_length: ::core::default::Default::default(),
                        validator_set_hash: ::core::default::Default::default(),
                        required_stake: ::core::default::Default::default(),
                        withdrawals_root: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for BlockHeader {}
            #[automatically_derived]
            impl ::core::cmp::Eq for BlockHeader {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<u32>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<u16>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                }
            }
            impl ::core::marker::StructuralPartialEq for BlockHeader {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for BlockHeader {
                #[inline]
                fn eq(&self, other: &BlockHeader) -> bool {
                    self.producer == other.producer
                        && self.previous_block_root == other.previous_block_root
                        && self.height == other.height
                        && self.block_number == other.block_number
                        && self.digest_root == other.digest_root
                        && self.digest_hash == other.digest_hash
                        && self.digest_length == other.digest_length
                        && self.transaction_root == other.transaction_root
                        && self.transaction_sum == other.transaction_sum
                        && self.transaction_hash == other.transaction_hash
                        && self.num_transactions == other.num_transactions
                        && self.transactions_data_length
                            == other.transactions_data_length
                        && self.validator_set_hash == other.validator_set_hash
                        && self.required_stake == other.required_stake
                        && self.withdrawals_root == other.withdrawals_root
                }
            }
            impl ethers_core::abi::AbiType for BlockHeader {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <u16 as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                                <u32 as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for BlockHeader {}
            impl ethers_core::abi::Tokenizable for BlockHeader
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                u16: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 15usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&15usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            producer: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            previous_block_root: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            height: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            block_number: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            digest_root: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            digest_hash: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            digest_length: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            transaction_root: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            transaction_sum: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            transaction_hash: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            num_transactions: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            transactions_data_length: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            validator_set_hash: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            required_stake: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            withdrawals_root: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.producer.into_token(),
                                self.previous_block_root.into_token(),
                                self.height.into_token(),
                                self.block_number.into_token(),
                                self.digest_root.into_token(),
                                self.digest_hash.into_token(),
                                self.digest_length.into_token(),
                                self.transaction_root.into_token(),
                                self.transaction_sum.into_token(),
                                self.transaction_hash.into_token(),
                                self.num_transactions.into_token(),
                                self.transactions_data_length.into_token(),
                                self.validator_set_hash.into_token(),
                                self.required_stake.into_token(),
                                self.withdrawals_root.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for BlockHeader
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                u16: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
                u32: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for BlockHeader {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for BlockHeader {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///`Withdrawal(address,address,uint256,uint8,uint256)`
            pub struct Withdrawal {
                pub owner: ethers_core::types::Address,
                pub token: ethers_core::types::Address,
                pub amount: ethers_core::types::U256,
                pub precision: u8,
                pub nonce: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for Withdrawal {
                #[inline]
                fn clone(&self) -> Withdrawal {
                    Withdrawal {
                        owner: ::core::clone::Clone::clone(&self.owner),
                        token: ::core::clone::Clone::clone(&self.token),
                        amount: ::core::clone::Clone::clone(&self.amount),
                        precision: ::core::clone::Clone::clone(&self.precision),
                        nonce: ::core::clone::Clone::clone(&self.nonce),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for Withdrawal {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field5_finish(
                        f,
                        "Withdrawal",
                        "owner",
                        &&self.owner,
                        "token",
                        &&self.token,
                        "amount",
                        &&self.amount,
                        "precision",
                        &&self.precision,
                        "nonce",
                        &&self.nonce,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for Withdrawal {
                #[inline]
                fn default() -> Withdrawal {
                    Withdrawal {
                        owner: ::core::default::Default::default(),
                        token: ::core::default::Default::default(),
                        amount: ::core::default::Default::default(),
                        precision: ::core::default::Default::default(),
                        nonce: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for Withdrawal {}
            #[automatically_derived]
            impl ::core::cmp::Eq for Withdrawal {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                    let _: ::core::cmp::AssertParamIsEq<u8>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for Withdrawal {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for Withdrawal {
                #[inline]
                fn eq(&self, other: &Withdrawal) -> bool {
                    self.owner == other.owner && self.token == other.token
                        && self.amount == other.amount
                        && self.precision == other.precision && self.nonce == other.nonce
                }
            }
            impl ethers_core::abi::AbiType for Withdrawal {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                                <u8 as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for Withdrawal {}
            impl ethers_core::abi::Tokenizable for Withdrawal
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
                u8: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 5usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&5usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            owner: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            token: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            precision: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            nonce: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.owner.into_token(),
                                self.token.into_token(),
                                self.amount.into_token(),
                                self.precision.into_token(),
                                self.nonce.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for Withdrawal
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
                u8: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for Withdrawal {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for Withdrawal {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
        }
    }
    pub mod validators {
        pub use validator_set::*;
        #[allow(clippy::too_many_arguments, non_camel_case_types)]
        pub mod validator_set {
            #![allow(clippy::enum_variant_names)]
            #![allow(dead_code)]
            #![allow(clippy::type_complexity)]
            #![allow(unused_imports)]
            ///ValidatorSet was auto-generated with ethers-rs Abigen. More information at: https://github.com/gakonst/ethers-rs
            use std::sync::Arc;
            use ethers_core::{
                abi::{Abi, Token, Detokenize, InvalidOutputType, Tokenizable},
                types::*,
            };
            use ethers_contract::{
                Contract, builders::{ContractCall, Event},
                Lazy,
            };
            use ethers_providers::Middleware;
            pub static VALIDATORSET_ABI: ethers_contract::Lazy<ethers_core::abi::Abi> = ethers_contract::Lazy::new(||
            {
                ethers_core::utils::__serde_json::from_str(
                        "[{\"inputs\":[{\"internalType\":\"address\",\"name\":\"tokenAddress\",\"type\":\"address\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"minStake\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"lockup\",\"type\":\"uint256\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"constructor\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"address\",\"name\":\"delegator\",\"type\":\"address\",\"components\":[],\"indexed\":true},{\"internalType\":\"bytes[]\",\"name\":\"delegates\",\"type\":\"bytes[]\",\"components\":[],\"indexed\":false},{\"internalType\":\"uint256[]\",\"name\":\"amounts\",\"type\":\"uint256[]\",\"components\":[],\"indexed\":false}],\"type\":\"event\",\"name\":\"Delegation\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"address\",\"name\":\"depositor\",\"type\":\"address\",\"components\":[],\"indexed\":true},{\"internalType\":\"uint256\",\"name\":\"amount\",\"type\":\"uint256\",\"components\":[],\"indexed\":true}],\"type\":\"event\",\"name\":\"Deposit\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"bytes\",\"name\":\"stakingKey\",\"type\":\"bytes\",\"components\":[],\"indexed\":true},{\"internalType\":\"bytes\",\"name\":\"consensusKey\",\"type\":\"bytes\",\"components\":[],\"indexed\":true}],\"type\":\"event\",\"name\":\"ValidatorRegistration\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"bytes\",\"name\":\"stakingKey\",\"type\":\"bytes\",\"components\":[],\"indexed\":true}],\"type\":\"event\",\"name\":\"ValidatorUnregistration\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"address\",\"name\":\"withdrawer\",\"type\":\"address\",\"components\":[],\"indexed\":true},{\"internalType\":\"uint256\",\"name\":\"amount\",\"type\":\"uint256\",\"components\":[],\"indexed\":true}],\"type\":\"event\",\"name\":\"Withdrawal\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"LOCKUP\",\"outputs\":[{\"internalType\":\"uint256\",\"name\":\"\",\"type\":\"uint256\",\"components\":[]}]},{\"inputs\":[],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"MIN_STAKE\",\"outputs\":[{\"internalType\":\"uint256\",\"name\":\"\",\"type\":\"uint256\",\"components\":[]}]},{\"inputs\":[],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"TOKEN_ADDRESS\",\"outputs\":[{\"internalType\":\"address\",\"name\":\"\",\"type\":\"address\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"bytes[]\",\"name\":\"delegates\",\"type\":\"bytes[]\",\"components\":[]},{\"internalType\":\"uint256[]\",\"name\":\"amounts\",\"type\":\"uint256[]\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"delegate\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"uint256\",\"name\":\"amount\",\"type\":\"uint256\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"deposit\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bytes\",\"name\":\"stakingKey\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensusKey\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"signature\",\"type\":\"bytes\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"registerValidator\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"address\",\"name\":\"\",\"type\":\"address\",\"components\":[]}],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"s_deposit\",\"outputs\":[{\"internalType\":\"uint256\",\"name\":\"amount\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"lockup\",\"type\":\"uint256\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"bytes\",\"name\":\"stakingKey\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"signature\",\"type\":\"bytes\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"unregisterValidator\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"uint256\",\"name\":\"amount\",\"type\":\"uint256\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"withdraw\",\"outputs\":[]}]",
                    )
                    .expect("invalid abi")
            });
            pub struct ValidatorSet<M>(ethers_contract::Contract<M>);
            impl<M> Clone for ValidatorSet<M> {
                fn clone(&self) -> Self {
                    ValidatorSet(self.0.clone())
                }
            }
            impl<M> std::ops::Deref for ValidatorSet<M> {
                type Target = ethers_contract::Contract<M>;
                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }
            impl<M: ethers_providers::Middleware> std::fmt::Debug for ValidatorSet<M> {
                fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.debug_tuple("ValidatorSet").field(&self.address()).finish()
                }
            }
            impl<M: ethers_providers::Middleware> ValidatorSet<M> {
                /// Creates a new contract instance with the specified `ethers`
                /// client at the given `Address`. The contract derefs to a `ethers::Contract`
                /// object
                pub fn new<T: Into<ethers_core::types::Address>>(
                    address: T,
                    client: ::std::sync::Arc<M>,
                ) -> Self {
                    ethers_contract::Contract::new(
                            address.into(),
                            VALIDATORSET_ABI.clone(),
                            client,
                        )
                        .into()
                }
                ///Calls the contract's `LOCKUP` (0x845aef4b) function
                pub fn lockup(
                    &self,
                ) -> ethers_contract::builders::ContractCall<
                    M,
                    ethers_core::types::U256,
                > {
                    self.0
                        .method_hash([132, 90, 239, 75], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `MIN_STAKE` (0xcb1c2b5c) function
                pub fn min_stake(
                    &self,
                ) -> ethers_contract::builders::ContractCall<
                    M,
                    ethers_core::types::U256,
                > {
                    self.0
                        .method_hash([203, 28, 43, 92], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `TOKEN_ADDRESS` (0x0bdf5300) function
                pub fn token_address(
                    &self,
                ) -> ethers_contract::builders::ContractCall<
                    M,
                    ethers_core::types::Address,
                > {
                    self.0
                        .method_hash([11, 223, 83, 0], ())
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `delegate` (0x2a658d2b) function
                pub fn delegate(
                    &self,
                    delegates: ::std::vec::Vec<ethers_core::types::Bytes>,
                    amounts: ::std::vec::Vec<ethers_core::types::U256>,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash([42, 101, 141, 43], (delegates, amounts))
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `deposit` (0xb6b55f25) function
                pub fn deposit(
                    &self,
                    amount: ethers_core::types::U256,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash([182, 181, 95, 37], amount)
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `registerValidator` (0xea684f77) function
                pub fn register_validator(
                    &self,
                    staking_key: ethers_core::types::Bytes,
                    consensus_key: ethers_core::types::Bytes,
                    signature: ethers_core::types::Bytes,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash(
                            [234, 104, 79, 119],
                            (staking_key, consensus_key, signature),
                        )
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `s_deposit` (0xa99bdd1c) function
                pub fn s_deposit(
                    &self,
                    p0: ethers_core::types::Address,
                ) -> ethers_contract::builders::ContractCall<
                    M,
                    (ethers_core::types::U256, ethers_core::types::U256),
                > {
                    self.0
                        .method_hash([169, 155, 221, 28], p0)
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `unregisterValidator` (0x5c64995c) function
                pub fn unregister_validator(
                    &self,
                    staking_key: ethers_core::types::Bytes,
                    signature: ethers_core::types::Bytes,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash([92, 100, 153, 92], (staking_key, signature))
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `withdraw` (0x2e1a7d4d) function
                pub fn withdraw(
                    &self,
                    amount: ethers_core::types::U256,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash([46, 26, 125, 77], amount)
                        .expect("method not found (this should never happen)")
                }
                ///Gets the contract's `Delegation` event
                pub fn delegation_filter(
                    &self,
                ) -> ethers_contract::builders::Event<M, DelegationFilter> {
                    self.0.event()
                }
                ///Gets the contract's `Deposit` event
                pub fn deposit_filter(
                    &self,
                ) -> ethers_contract::builders::Event<M, DepositFilter> {
                    self.0.event()
                }
                ///Gets the contract's `ValidatorRegistration` event
                pub fn validator_registration_filter(
                    &self,
                ) -> ethers_contract::builders::Event<M, ValidatorRegistrationFilter> {
                    self.0.event()
                }
                ///Gets the contract's `ValidatorUnregistration` event
                pub fn validator_unregistration_filter(
                    &self,
                ) -> ethers_contract::builders::Event<M, ValidatorUnregistrationFilter> {
                    self.0.event()
                }
                ///Gets the contract's `Withdrawal` event
                pub fn withdrawal_filter(
                    &self,
                ) -> ethers_contract::builders::Event<M, WithdrawalFilter> {
                    self.0.event()
                }
                /// Returns an [`Event`](#ethers_contract::builders::Event) builder for all events of this contract
                pub fn events(
                    &self,
                ) -> ethers_contract::builders::Event<M, ValidatorSetEvents> {
                    self.0.event_with_filter(Default::default())
                }
            }
            impl<M: ethers_providers::Middleware> From<ethers_contract::Contract<M>>
            for ValidatorSet<M> {
                fn from(contract: ethers_contract::Contract<M>) -> Self {
                    Self(contract)
                }
            }
            #[ethevent(
                name = "Delegation",
                abi = "Delegation(address,bytes[],uint256[])"
            )]
            pub struct DelegationFilter {
                #[ethevent(indexed)]
                pub delegator: ethers_core::types::Address,
                pub delegates: Vec<ethers_core::types::Bytes>,
                pub amounts: Vec<ethers_core::types::U256>,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for DelegationFilter {
                #[inline]
                fn clone(&self) -> DelegationFilter {
                    DelegationFilter {
                        delegator: ::core::clone::Clone::clone(&self.delegator),
                        delegates: ::core::clone::Clone::clone(&self.delegates),
                        amounts: ::core::clone::Clone::clone(&self.amounts),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for DelegationFilter {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field3_finish(
                        f,
                        "DelegationFilter",
                        "delegator",
                        &&self.delegator,
                        "delegates",
                        &&self.delegates,
                        "amounts",
                        &&self.amounts,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for DelegationFilter {
                #[inline]
                fn default() -> DelegationFilter {
                    DelegationFilter {
                        delegator: ::core::default::Default::default(),
                        delegates: ::core::default::Default::default(),
                        amounts: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for DelegationFilter {}
            #[automatically_derived]
            impl ::core::cmp::Eq for DelegationFilter {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<Vec<ethers_core::types::Bytes>>;
                }
            }
            impl ::core::marker::StructuralPartialEq for DelegationFilter {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for DelegationFilter {
                #[inline]
                fn eq(&self, other: &DelegationFilter) -> bool {
                    self.delegator == other.delegator
                        && self.delegates == other.delegates
                        && self.amounts == other.amounts
                }
            }
            impl ethers_core::abi::AbiType for DelegationFilter {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <Vec<
                                    ethers_core::types::Bytes,
                                > as ethers_core::abi::AbiType>::param_type(),
                                <Vec<
                                    ethers_core::types::U256,
                                > as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for DelegationFilter {}
            impl ethers_core::abi::Tokenizable for DelegationFilter
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                Vec<ethers_core::types::Bytes>: ethers_core::abi::Tokenize,
                Vec<ethers_core::types::U256>: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 3usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&3usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            delegator: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            delegates: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amounts: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.delegator.into_token(),
                                self.delegates.into_token(),
                                self.amounts.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for DelegationFilter
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                Vec<ethers_core::types::Bytes>: ethers_core::abi::Tokenize,
                Vec<ethers_core::types::U256>: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthEvent for DelegationFilter {
                fn name() -> ::std::borrow::Cow<'static, str> {
                    "Delegation".into()
                }
                fn signature() -> ethers_core::types::H256 {
                    ethers_core::types::H256([
                        179,
                        4,
                        36,
                        60,
                        91,
                        84,
                        101,
                        160,
                        246,
                        166,
                        180,
                        75,
                        228,
                        91,
                        105,
                        6,
                        101,
                        13,
                        84,
                        44,
                        142,
                        29,
                        211,
                        59,
                        6,
                        48,
                        247,
                        43,
                        47,
                        69,
                        64,
                        129,
                    ])
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "Delegation(address,bytes[],uint256[])".into()
                }
                fn decode_log(
                    log: &ethers_core::abi::RawLog,
                ) -> Result<Self, ethers_core::abi::Error>
                where
                    Self: Sized,
                {
                    let ethers_core::abi::RawLog { data, topics } = log;
                    let event_signature = topics
                        .get(0)
                        .ok_or(ethers_core::abi::Error::InvalidData)?;
                    if event_signature != &Self::signature() {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let topic_types = <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([ethers_core::abi::ParamType::Address]),
                    );
                    let data_types = [
                        ethers_core::abi::ParamType::Array(
                            Box::new(ethers_core::abi::ParamType::Bytes),
                        ),
                        ethers_core::abi::ParamType::Array(
                            Box::new(ethers_core::abi::ParamType::Uint(256usize)),
                        ),
                    ];
                    let flat_topics = topics
                        .iter()
                        .skip(1)
                        .flat_map(|t| t.as_ref().to_vec())
                        .collect::<Vec<u8>>();
                    let topic_tokens = ethers_core::abi::decode(
                        &topic_types,
                        &flat_topics,
                    )?;
                    if topic_tokens.len() != topics.len() - 1 {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let data_tokens = ethers_core::abi::decode(&data_types, data)?;
                    let tokens: Vec<_> = topic_tokens
                        .into_iter()
                        .chain(data_tokens.into_iter())
                        .collect();
                    ethers_core::abi::Tokenizable::from_token(
                            ethers_core::abi::Token::Tuple(tokens),
                        )
                        .map_err(|_| ethers_core::abi::Error::InvalidData)
                }
                fn is_anonymous() -> bool {
                    false
                }
            }
            impl ::std::fmt::Display for DelegationFilter {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.delegator)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&["["], &[]))?;
                    for (idx, val) in self.delegates.iter().enumerate() {
                        f.write_fmt(
                            ::core::fmt::Arguments::new_v1(
                                &[""],
                                &[::core::fmt::ArgumentV1::new_debug(&val)],
                            ),
                        )?;
                        if idx < self.delegates.len() - 1 {
                            f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                        }
                    }
                    f.write_fmt(::core::fmt::Arguments::new_v1(&["]"], &[]))?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&["["], &[]))?;
                    for (idx, val) in self.amounts.iter().enumerate() {
                        f.write_fmt(
                            ::core::fmt::Arguments::new_v1(
                                &[""],
                                &[::core::fmt::ArgumentV1::new_debug(&val)],
                            ),
                        )?;
                        if idx < self.amounts.len() - 1 {
                            f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                        }
                    }
                    f.write_fmt(::core::fmt::Arguments::new_v1(&["]"], &[]))?;
                    Ok(())
                }
            }
            #[ethevent(name = "Deposit", abi = "Deposit(address,uint256)")]
            pub struct DepositFilter {
                #[ethevent(indexed)]
                pub depositor: ethers_core::types::Address,
                #[ethevent(indexed)]
                pub amount: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for DepositFilter {
                #[inline]
                fn clone(&self) -> DepositFilter {
                    DepositFilter {
                        depositor: ::core::clone::Clone::clone(&self.depositor),
                        amount: ::core::clone::Clone::clone(&self.amount),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for DepositFilter {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "DepositFilter",
                        "depositor",
                        &&self.depositor,
                        "amount",
                        &&self.amount,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for DepositFilter {
                #[inline]
                fn default() -> DepositFilter {
                    DepositFilter {
                        depositor: ::core::default::Default::default(),
                        amount: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for DepositFilter {}
            #[automatically_derived]
            impl ::core::cmp::Eq for DepositFilter {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for DepositFilter {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for DepositFilter {
                #[inline]
                fn eq(&self, other: &DepositFilter) -> bool {
                    self.depositor == other.depositor && self.amount == other.amount
                }
            }
            impl ethers_core::abi::AbiType for DepositFilter {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for DepositFilter {}
            impl ethers_core::abi::Tokenizable for DepositFilter
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            depositor: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.depositor.into_token(),
                                self.amount.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for DepositFilter
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthEvent for DepositFilter {
                fn name() -> ::std::borrow::Cow<'static, str> {
                    "Deposit".into()
                }
                fn signature() -> ethers_core::types::H256 {
                    ethers_core::types::H256([
                        225,
                        255,
                        252,
                        196,
                        146,
                        61,
                        4,
                        181,
                        89,
                        244,
                        210,
                        154,
                        139,
                        252,
                        108,
                        218,
                        4,
                        235,
                        91,
                        13,
                        60,
                        70,
                        7,
                        81,
                        194,
                        64,
                        44,
                        92,
                        92,
                        201,
                        16,
                        156,
                    ])
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "Deposit(address,uint256)".into()
                }
                fn decode_log(
                    log: &ethers_core::abi::RawLog,
                ) -> Result<Self, ethers_core::abi::Error>
                where
                    Self: Sized,
                {
                    let ethers_core::abi::RawLog { data, topics } = log;
                    let event_signature = topics
                        .get(0)
                        .ok_or(ethers_core::abi::Error::InvalidData)?;
                    if event_signature != &Self::signature() {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let topic_types = <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            ethers_core::abi::ParamType::Address,
                            ethers_core::abi::ParamType::Uint(256usize),
                        ]),
                    );
                    let data_types = [];
                    let flat_topics = topics
                        .iter()
                        .skip(1)
                        .flat_map(|t| t.as_ref().to_vec())
                        .collect::<Vec<u8>>();
                    let topic_tokens = ethers_core::abi::decode(
                        &topic_types,
                        &flat_topics,
                    )?;
                    if topic_tokens.len() != topics.len() - 1 {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let data_tokens = ethers_core::abi::decode(&data_types, data)?;
                    let tokens: Vec<_> = topic_tokens
                        .into_iter()
                        .chain(data_tokens.into_iter())
                        .collect();
                    ethers_core::abi::Tokenizable::from_token(
                            ethers_core::abi::Token::Tuple(tokens),
                        )
                        .map_err(|_| ethers_core::abi::Error::InvalidData)
                }
                fn is_anonymous() -> bool {
                    false
                }
            }
            impl ::std::fmt::Display for DepositFilter {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.depositor)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.amount)],
                        ),
                    )?;
                    Ok(())
                }
            }
            #[ethevent(
                name = "ValidatorRegistration",
                abi = "ValidatorRegistration(bytes,bytes)"
            )]
            pub struct ValidatorRegistrationFilter {
                #[ethevent(indexed)]
                pub staking_key: ethers_core::types::H256,
                #[ethevent(indexed)]
                pub consensus_key: ethers_core::types::H256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for ValidatorRegistrationFilter {
                #[inline]
                fn clone(&self) -> ValidatorRegistrationFilter {
                    ValidatorRegistrationFilter {
                        staking_key: ::core::clone::Clone::clone(&self.staking_key),
                        consensus_key: ::core::clone::Clone::clone(&self.consensus_key),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for ValidatorRegistrationFilter {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "ValidatorRegistrationFilter",
                        "staking_key",
                        &&self.staking_key,
                        "consensus_key",
                        &&self.consensus_key,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for ValidatorRegistrationFilter {
                #[inline]
                fn default() -> ValidatorRegistrationFilter {
                    ValidatorRegistrationFilter {
                        staking_key: ::core::default::Default::default(),
                        consensus_key: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for ValidatorRegistrationFilter {}
            #[automatically_derived]
            impl ::core::cmp::Eq for ValidatorRegistrationFilter {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::H256>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::H256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for ValidatorRegistrationFilter {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for ValidatorRegistrationFilter {
                #[inline]
                fn eq(&self, other: &ValidatorRegistrationFilter) -> bool {
                    self.staking_key == other.staking_key
                        && self.consensus_key == other.consensus_key
                }
            }
            impl ethers_core::abi::AbiType for ValidatorRegistrationFilter {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::H256 as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::H256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for ValidatorRegistrationFilter {}
            impl ethers_core::abi::Tokenizable for ValidatorRegistrationFilter
            where
                ethers_core::types::H256: ethers_core::abi::Tokenize,
                ethers_core::types::H256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            staking_key: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            consensus_key: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.staking_key.into_token(),
                                self.consensus_key.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for ValidatorRegistrationFilter
            where
                ethers_core::types::H256: ethers_core::abi::Tokenize,
                ethers_core::types::H256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthEvent for ValidatorRegistrationFilter {
                fn name() -> ::std::borrow::Cow<'static, str> {
                    "ValidatorRegistration".into()
                }
                fn signature() -> ethers_core::types::H256 {
                    ethers_core::types::H256([
                        184,
                        128,
                        174,
                        154,
                        65,
                        198,
                        122,
                        182,
                        30,
                        103,
                        9,
                        41,
                        152,
                        62,
                        163,
                        131,
                        129,
                        15,
                        42,
                        9,
                        227,
                        132,
                        181,
                        209,
                        228,
                        10,
                        106,
                        141,
                        18,
                        62,
                        100,
                        63,
                    ])
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "ValidatorRegistration(bytes,bytes)".into()
                }
                fn decode_log(
                    log: &ethers_core::abi::RawLog,
                ) -> Result<Self, ethers_core::abi::Error>
                where
                    Self: Sized,
                {
                    let ethers_core::abi::RawLog { data, topics } = log;
                    let event_signature = topics
                        .get(0)
                        .ok_or(ethers_core::abi::Error::InvalidData)?;
                    if event_signature != &Self::signature() {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let topic_types = <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            ethers_core::abi::ParamType::FixedBytes(32),
                            ethers_core::abi::ParamType::FixedBytes(32),
                        ]),
                    );
                    let data_types = [];
                    let flat_topics = topics
                        .iter()
                        .skip(1)
                        .flat_map(|t| t.as_ref().to_vec())
                        .collect::<Vec<u8>>();
                    let topic_tokens = ethers_core::abi::decode(
                        &topic_types,
                        &flat_topics,
                    )?;
                    if topic_tokens.len() != topics.len() - 1 {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let data_tokens = ethers_core::abi::decode(&data_types, data)?;
                    let tokens: Vec<_> = topic_tokens
                        .into_iter()
                        .chain(data_tokens.into_iter())
                        .collect();
                    ethers_core::abi::Tokenizable::from_token(
                            ethers_core::abi::Token::Tuple(tokens),
                        )
                        .map_err(|_| ethers_core::abi::Error::InvalidData)
                }
                fn is_anonymous() -> bool {
                    false
                }
            }
            impl ::std::fmt::Display for ValidatorRegistrationFilter {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.staking_key),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.consensus_key),
                                ),
                            ],
                        ),
                    )?;
                    Ok(())
                }
            }
            #[ethevent(
                name = "ValidatorUnregistration",
                abi = "ValidatorUnregistration(bytes)"
            )]
            pub struct ValidatorUnregistrationFilter {
                #[ethevent(indexed)]
                pub staking_key: ethers_core::types::H256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for ValidatorUnregistrationFilter {
                #[inline]
                fn clone(&self) -> ValidatorUnregistrationFilter {
                    ValidatorUnregistrationFilter {
                        staking_key: ::core::clone::Clone::clone(&self.staking_key),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for ValidatorUnregistrationFilter {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field1_finish(
                        f,
                        "ValidatorUnregistrationFilter",
                        "staking_key",
                        &&self.staking_key,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for ValidatorUnregistrationFilter {
                #[inline]
                fn default() -> ValidatorUnregistrationFilter {
                    ValidatorUnregistrationFilter {
                        staking_key: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for ValidatorUnregistrationFilter {}
            #[automatically_derived]
            impl ::core::cmp::Eq for ValidatorUnregistrationFilter {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::H256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for ValidatorUnregistrationFilter {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for ValidatorUnregistrationFilter {
                #[inline]
                fn eq(&self, other: &ValidatorUnregistrationFilter) -> bool {
                    self.staking_key == other.staking_key
                }
            }
            impl ethers_core::abi::AbiType for ValidatorUnregistrationFilter {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::H256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for ValidatorUnregistrationFilter {}
            impl ethers_core::abi::Tokenizable for ValidatorUnregistrationFilter
            where
                ethers_core::types::H256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            staking_key: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.staking_key.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for ValidatorUnregistrationFilter
            where
                ethers_core::types::H256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthEvent for ValidatorUnregistrationFilter {
                fn name() -> ::std::borrow::Cow<'static, str> {
                    "ValidatorUnregistration".into()
                }
                fn signature() -> ethers_core::types::H256 {
                    ethers_core::types::H256([
                        244,
                        125,
                        177,
                        15,
                        90,
                        250,
                        67,
                        179,
                        28,
                        138,
                        137,
                        123,
                        178,
                        81,
                        100,
                        26,
                        125,
                        43,
                        132,
                        1,
                        28,
                        86,
                        124,
                        144,
                        106,
                        93,
                        243,
                        71,
                        193,
                        131,
                        223,
                        20,
                    ])
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "ValidatorUnregistration(bytes)".into()
                }
                fn decode_log(
                    log: &ethers_core::abi::RawLog,
                ) -> Result<Self, ethers_core::abi::Error>
                where
                    Self: Sized,
                {
                    let ethers_core::abi::RawLog { data, topics } = log;
                    let event_signature = topics
                        .get(0)
                        .ok_or(ethers_core::abi::Error::InvalidData)?;
                    if event_signature != &Self::signature() {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let topic_types = <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            ethers_core::abi::ParamType::FixedBytes(32),
                        ]),
                    );
                    let data_types = [];
                    let flat_topics = topics
                        .iter()
                        .skip(1)
                        .flat_map(|t| t.as_ref().to_vec())
                        .collect::<Vec<u8>>();
                    let topic_tokens = ethers_core::abi::decode(
                        &topic_types,
                        &flat_topics,
                    )?;
                    if topic_tokens.len() != topics.len() - 1 {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let data_tokens = ethers_core::abi::decode(&data_types, data)?;
                    let tokens: Vec<_> = topic_tokens
                        .into_iter()
                        .chain(data_tokens.into_iter())
                        .collect();
                    ethers_core::abi::Tokenizable::from_token(
                            ethers_core::abi::Token::Tuple(tokens),
                        )
                        .map_err(|_| ethers_core::abi::Error::InvalidData)
                }
                fn is_anonymous() -> bool {
                    false
                }
            }
            impl ::std::fmt::Display for ValidatorUnregistrationFilter {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.staking_key),
                                ),
                            ],
                        ),
                    )?;
                    Ok(())
                }
            }
            #[ethevent(name = "Withdrawal", abi = "Withdrawal(address,uint256)")]
            pub struct WithdrawalFilter {
                #[ethevent(indexed)]
                pub withdrawer: ethers_core::types::Address,
                #[ethevent(indexed)]
                pub amount: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for WithdrawalFilter {
                #[inline]
                fn clone(&self) -> WithdrawalFilter {
                    WithdrawalFilter {
                        withdrawer: ::core::clone::Clone::clone(&self.withdrawer),
                        amount: ::core::clone::Clone::clone(&self.amount),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for WithdrawalFilter {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "WithdrawalFilter",
                        "withdrawer",
                        &&self.withdrawer,
                        "amount",
                        &&self.amount,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for WithdrawalFilter {
                #[inline]
                fn default() -> WithdrawalFilter {
                    WithdrawalFilter {
                        withdrawer: ::core::default::Default::default(),
                        amount: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for WithdrawalFilter {}
            #[automatically_derived]
            impl ::core::cmp::Eq for WithdrawalFilter {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for WithdrawalFilter {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for WithdrawalFilter {
                #[inline]
                fn eq(&self, other: &WithdrawalFilter) -> bool {
                    self.withdrawer == other.withdrawer && self.amount == other.amount
                }
            }
            impl ethers_core::abi::AbiType for WithdrawalFilter {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for WithdrawalFilter {}
            impl ethers_core::abi::Tokenizable for WithdrawalFilter
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            withdrawer: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.withdrawer.into_token(),
                                self.amount.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for WithdrawalFilter
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthEvent for WithdrawalFilter {
                fn name() -> ::std::borrow::Cow<'static, str> {
                    "Withdrawal".into()
                }
                fn signature() -> ethers_core::types::H256 {
                    ethers_core::types::H256([
                        127,
                        207,
                        83,
                        44,
                        21,
                        240,
                        166,
                        219,
                        11,
                        214,
                        208,
                        224,
                        56,
                        190,
                        167,
                        29,
                        48,
                        216,
                        8,
                        199,
                        217,
                        140,
                        179,
                        191,
                        114,
                        104,
                        169,
                        91,
                        245,
                        8,
                        27,
                        101,
                    ])
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "Withdrawal(address,uint256)".into()
                }
                fn decode_log(
                    log: &ethers_core::abi::RawLog,
                ) -> Result<Self, ethers_core::abi::Error>
                where
                    Self: Sized,
                {
                    let ethers_core::abi::RawLog { data, topics } = log;
                    let event_signature = topics
                        .get(0)
                        .ok_or(ethers_core::abi::Error::InvalidData)?;
                    if event_signature != &Self::signature() {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let topic_types = <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            ethers_core::abi::ParamType::Address,
                            ethers_core::abi::ParamType::Uint(256usize),
                        ]),
                    );
                    let data_types = [];
                    let flat_topics = topics
                        .iter()
                        .skip(1)
                        .flat_map(|t| t.as_ref().to_vec())
                        .collect::<Vec<u8>>();
                    let topic_tokens = ethers_core::abi::decode(
                        &topic_types,
                        &flat_topics,
                    )?;
                    if topic_tokens.len() != topics.len() - 1 {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let data_tokens = ethers_core::abi::decode(&data_types, data)?;
                    let tokens: Vec<_> = topic_tokens
                        .into_iter()
                        .chain(data_tokens.into_iter())
                        .collect();
                    ethers_core::abi::Tokenizable::from_token(
                            ethers_core::abi::Token::Tuple(tokens),
                        )
                        .map_err(|_| ethers_core::abi::Error::InvalidData)
                }
                fn is_anonymous() -> bool {
                    false
                }
            }
            impl ::std::fmt::Display for WithdrawalFilter {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.withdrawer)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.amount)],
                        ),
                    )?;
                    Ok(())
                }
            }
            pub enum ValidatorSetEvents {
                DelegationFilter(DelegationFilter),
                DepositFilter(DepositFilter),
                ValidatorRegistrationFilter(ValidatorRegistrationFilter),
                ValidatorUnregistrationFilter(ValidatorUnregistrationFilter),
                WithdrawalFilter(WithdrawalFilter),
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for ValidatorSetEvents {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        ValidatorSetEvents::DelegationFilter(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "DelegationFilter",
                                &__self_0,
                            )
                        }
                        ValidatorSetEvents::DepositFilter(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "DepositFilter",
                                &__self_0,
                            )
                        }
                        ValidatorSetEvents::ValidatorRegistrationFilter(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "ValidatorRegistrationFilter",
                                &__self_0,
                            )
                        }
                        ValidatorSetEvents::ValidatorUnregistrationFilter(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "ValidatorUnregistrationFilter",
                                &__self_0,
                            )
                        }
                        ValidatorSetEvents::WithdrawalFilter(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "WithdrawalFilter",
                                &__self_0,
                            )
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::clone::Clone for ValidatorSetEvents {
                #[inline]
                fn clone(&self) -> ValidatorSetEvents {
                    match self {
                        ValidatorSetEvents::DelegationFilter(__self_0) => {
                            ValidatorSetEvents::DelegationFilter(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetEvents::DepositFilter(__self_0) => {
                            ValidatorSetEvents::DepositFilter(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetEvents::ValidatorRegistrationFilter(__self_0) => {
                            ValidatorSetEvents::ValidatorRegistrationFilter(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetEvents::ValidatorUnregistrationFilter(__self_0) => {
                            ValidatorSetEvents::ValidatorUnregistrationFilter(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetEvents::WithdrawalFilter(__self_0) => {
                            ValidatorSetEvents::WithdrawalFilter(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                    }
                }
            }
            impl ::core::marker::StructuralPartialEq for ValidatorSetEvents {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for ValidatorSetEvents {
                #[inline]
                fn eq(&self, other: &ValidatorSetEvents) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                        && match (self, other) {
                            (
                                ValidatorSetEvents::DelegationFilter(__self_0),
                                ValidatorSetEvents::DelegationFilter(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetEvents::DepositFilter(__self_0),
                                ValidatorSetEvents::DepositFilter(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetEvents::ValidatorRegistrationFilter(__self_0),
                                ValidatorSetEvents::ValidatorRegistrationFilter(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetEvents::ValidatorUnregistrationFilter(__self_0),
                                ValidatorSetEvents::ValidatorUnregistrationFilter(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetEvents::WithdrawalFilter(__self_0),
                                ValidatorSetEvents::WithdrawalFilter(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
            impl ::core::marker::StructuralEq for ValidatorSetEvents {}
            #[automatically_derived]
            impl ::core::cmp::Eq for ValidatorSetEvents {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<DelegationFilter>;
                    let _: ::core::cmp::AssertParamIsEq<DepositFilter>;
                    let _: ::core::cmp::AssertParamIsEq<ValidatorRegistrationFilter>;
                    let _: ::core::cmp::AssertParamIsEq<ValidatorUnregistrationFilter>;
                    let _: ::core::cmp::AssertParamIsEq<WithdrawalFilter>;
                }
            }
            impl ethers_core::abi::Tokenizable for ValidatorSetEvents {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let Ok(decoded) = DelegationFilter::from_token(token.clone()) {
                        return Ok(ValidatorSetEvents::DelegationFilter(decoded));
                    }
                    if let Ok(decoded) = DepositFilter::from_token(token.clone()) {
                        return Ok(ValidatorSetEvents::DepositFilter(decoded));
                    }
                    if let Ok(decoded)
                        = ValidatorRegistrationFilter::from_token(token.clone()) {
                        return Ok(
                            ValidatorSetEvents::ValidatorRegistrationFilter(decoded),
                        );
                    }
                    if let Ok(decoded)
                        = ValidatorUnregistrationFilter::from_token(token.clone()) {
                        return Ok(
                            ValidatorSetEvents::ValidatorUnregistrationFilter(decoded),
                        );
                    }
                    if let Ok(decoded) = WithdrawalFilter::from_token(token.clone()) {
                        return Ok(ValidatorSetEvents::WithdrawalFilter(decoded));
                    }
                    Err(
                        ethers_core::abi::InvalidOutputType(
                            "Failed to decode all type variants".to_string(),
                        ),
                    )
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    match self {
                        ValidatorSetEvents::DelegationFilter(element) => {
                            element.into_token()
                        }
                        ValidatorSetEvents::DepositFilter(element) => {
                            element.into_token()
                        }
                        ValidatorSetEvents::ValidatorRegistrationFilter(element) => {
                            element.into_token()
                        }
                        ValidatorSetEvents::ValidatorUnregistrationFilter(element) => {
                            element.into_token()
                        }
                        ValidatorSetEvents::WithdrawalFilter(element) => {
                            element.into_token()
                        }
                    }
                }
            }
            impl ethers_core::abi::TokenizableItem for ValidatorSetEvents {}
            impl ethers_contract::EthLogDecode for ValidatorSetEvents {
                fn decode_log(
                    log: &ethers_core::abi::RawLog,
                ) -> Result<Self, ethers_core::abi::Error>
                where
                    Self: Sized,
                {
                    if let Ok(decoded) = DelegationFilter::decode_log(log) {
                        return Ok(ValidatorSetEvents::DelegationFilter(decoded));
                    }
                    if let Ok(decoded) = DepositFilter::decode_log(log) {
                        return Ok(ValidatorSetEvents::DepositFilter(decoded));
                    }
                    if let Ok(decoded) = ValidatorRegistrationFilter::decode_log(log) {
                        return Ok(
                            ValidatorSetEvents::ValidatorRegistrationFilter(decoded),
                        );
                    }
                    if let Ok(decoded) = ValidatorUnregistrationFilter::decode_log(log) {
                        return Ok(
                            ValidatorSetEvents::ValidatorUnregistrationFilter(decoded),
                        );
                    }
                    if let Ok(decoded) = WithdrawalFilter::decode_log(log) {
                        return Ok(ValidatorSetEvents::WithdrawalFilter(decoded));
                    }
                    Err(ethers_core::abi::Error::InvalidData)
                }
            }
            impl ::std::fmt::Display for ValidatorSetEvents {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    match self {
                        ValidatorSetEvents::DelegationFilter(element) => element.fmt(f),
                        ValidatorSetEvents::DepositFilter(element) => element.fmt(f),
                        ValidatorSetEvents::ValidatorRegistrationFilter(element) => {
                            element.fmt(f)
                        }
                        ValidatorSetEvents::ValidatorUnregistrationFilter(element) => {
                            element.fmt(f)
                        }
                        ValidatorSetEvents::WithdrawalFilter(element) => element.fmt(f),
                    }
                }
            }
            ///Container type for all input parameters for the `LOCKUP` function with signature `LOCKUP()` and selector `[132, 90, 239, 75]`
            #[ethcall(name = "LOCKUP", abi = "LOCKUP()")]
            pub struct LockupCall;
            #[automatically_derived]
            impl ::core::clone::Clone for LockupCall {
                #[inline]
                fn clone(&self) -> LockupCall {
                    LockupCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for LockupCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "LockupCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for LockupCall {
                #[inline]
                fn default() -> LockupCall {
                    LockupCall {}
                }
            }
            impl ::core::marker::StructuralEq for LockupCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for LockupCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for LockupCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for LockupCall {
                #[inline]
                fn eq(&self, other: &LockupCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for LockupCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(LockupCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for LockupCall {}
            impl ethers_contract::EthCall for LockupCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "LOCKUP".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [132, 90, 239, 75]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "LOCKUP()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for LockupCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for LockupCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for LockupCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `MIN_STAKE` function with signature `MIN_STAKE()` and selector `[203, 28, 43, 92]`
            #[ethcall(name = "MIN_STAKE", abi = "MIN_STAKE()")]
            pub struct MinStakeCall;
            #[automatically_derived]
            impl ::core::clone::Clone for MinStakeCall {
                #[inline]
                fn clone(&self) -> MinStakeCall {
                    MinStakeCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MinStakeCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "MinStakeCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for MinStakeCall {
                #[inline]
                fn default() -> MinStakeCall {
                    MinStakeCall {}
                }
            }
            impl ::core::marker::StructuralEq for MinStakeCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MinStakeCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for MinStakeCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MinStakeCall {
                #[inline]
                fn eq(&self, other: &MinStakeCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for MinStakeCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(MinStakeCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for MinStakeCall {}
            impl ethers_contract::EthCall for MinStakeCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "MIN_STAKE".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [203, 28, 43, 92]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "MIN_STAKE()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for MinStakeCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for MinStakeCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for MinStakeCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `TOKEN_ADDRESS` function with signature `TOKEN_ADDRESS()` and selector `[11, 223, 83, 0]`
            #[ethcall(name = "TOKEN_ADDRESS", abi = "TOKEN_ADDRESS()")]
            pub struct TokenAddressCall;
            #[automatically_derived]
            impl ::core::clone::Clone for TokenAddressCall {
                #[inline]
                fn clone(&self) -> TokenAddressCall {
                    TokenAddressCall
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for TokenAddressCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, "TokenAddressCall")
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for TokenAddressCall {
                #[inline]
                fn default() -> TokenAddressCall {
                    TokenAddressCall {}
                }
            }
            impl ::core::marker::StructuralEq for TokenAddressCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for TokenAddressCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            impl ::core::marker::StructuralPartialEq for TokenAddressCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for TokenAddressCall {
                #[inline]
                fn eq(&self, other: &TokenAddressCall) -> bool {
                    true
                }
            }
            impl ethers_core::abi::Tokenizable for TokenAddressCall {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if !tokens.is_empty() {
                            Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected empty tuple, got "],
                                            &[::core::fmt::ArgumentV1::new_debug(&tokens)],
                                        ),
                                    );
                                    res
                                }),
                            )
                        } else {
                            Ok(TokenAddressCall {})
                        }
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(::std::vec::Vec::new())
                }
            }
            impl ethers_core::abi::TokenizableItem for TokenAddressCall {}
            impl ethers_contract::EthCall for TokenAddressCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "TOKEN_ADDRESS".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [11, 223, 83, 0]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "TOKEN_ADDRESS()".into()
                }
            }
            impl ethers_core::abi::AbiDecode for TokenAddressCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for TokenAddressCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for TokenAddressCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `delegate` function with signature `delegate(bytes[],uint256[])` and selector `[42, 101, 141, 43]`
            #[ethcall(name = "delegate", abi = "delegate(bytes[],uint256[])")]
            pub struct DelegateCall {
                pub delegates: ::std::vec::Vec<ethers_core::types::Bytes>,
                pub amounts: ::std::vec::Vec<ethers_core::types::U256>,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for DelegateCall {
                #[inline]
                fn clone(&self) -> DelegateCall {
                    DelegateCall {
                        delegates: ::core::clone::Clone::clone(&self.delegates),
                        amounts: ::core::clone::Clone::clone(&self.amounts),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for DelegateCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "DelegateCall",
                        "delegates",
                        &&self.delegates,
                        "amounts",
                        &&self.amounts,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for DelegateCall {
                #[inline]
                fn default() -> DelegateCall {
                    DelegateCall {
                        delegates: ::core::default::Default::default(),
                        amounts: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for DelegateCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for DelegateCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<
                        ::std::vec::Vec<ethers_core::types::Bytes>,
                    >;
                    let _: ::core::cmp::AssertParamIsEq<
                        ::std::vec::Vec<ethers_core::types::U256>,
                    >;
                }
            }
            impl ::core::marker::StructuralPartialEq for DelegateCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for DelegateCall {
                #[inline]
                fn eq(&self, other: &DelegateCall) -> bool {
                    self.delegates == other.delegates && self.amounts == other.amounts
                }
            }
            impl ethers_core::abi::AbiType for DelegateCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <::std::vec::Vec<
                                    ethers_core::types::Bytes,
                                > as ethers_core::abi::AbiType>::param_type(),
                                <::std::vec::Vec<
                                    ethers_core::types::U256,
                                > as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for DelegateCall {}
            impl ethers_core::abi::Tokenizable for DelegateCall
            where
                ::std::vec::Vec<ethers_core::types::Bytes>: ethers_core::abi::Tokenize,
                ::std::vec::Vec<ethers_core::types::U256>: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            delegates: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amounts: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.delegates.into_token(),
                                self.amounts.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for DelegateCall
            where
                ::std::vec::Vec<ethers_core::types::Bytes>: ethers_core::abi::Tokenize,
                ::std::vec::Vec<ethers_core::types::U256>: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for DelegateCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "delegate".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [42, 101, 141, 43]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "delegate(bytes[],uint256[])".into()
                }
            }
            impl ethers_core::abi::AbiDecode for DelegateCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::Array(
                            Box::new(ethers_core::abi::ParamType::Bytes),
                        ),
                        ethers_core::abi::ParamType::Array(
                            Box::new(ethers_core::abi::ParamType::Uint(256usize)),
                        ),
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for DelegateCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for DelegateCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&&self.delegates)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&&self.amounts)],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `deposit` function with signature `deposit(uint256)` and selector `[182, 181, 95, 37]`
            #[ethcall(name = "deposit", abi = "deposit(uint256)")]
            pub struct DepositCall {
                pub amount: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for DepositCall {
                #[inline]
                fn clone(&self) -> DepositCall {
                    DepositCall {
                        amount: ::core::clone::Clone::clone(&self.amount),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for DepositCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field1_finish(
                        f,
                        "DepositCall",
                        "amount",
                        &&self.amount,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for DepositCall {
                #[inline]
                fn default() -> DepositCall {
                    DepositCall {
                        amount: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for DepositCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for DepositCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for DepositCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for DepositCall {
                #[inline]
                fn eq(&self, other: &DepositCall) -> bool {
                    self.amount == other.amount
                }
            }
            impl ethers_core::abi::AbiType for DepositCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for DepositCall {}
            impl ethers_core::abi::Tokenizable for DepositCall
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.amount.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for DepositCall
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for DepositCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "deposit".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [182, 181, 95, 37]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "deposit(uint256)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for DepositCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [ethers_core::abi::ParamType::Uint(256usize)];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for DepositCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for DepositCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.amount)],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `registerValidator` function with signature `registerValidator(bytes,bytes,bytes)` and selector `[234, 104, 79, 119]`
            #[ethcall(
                name = "registerValidator",
                abi = "registerValidator(bytes,bytes,bytes)"
            )]
            pub struct RegisterValidatorCall {
                pub staking_key: ethers_core::types::Bytes,
                pub consensus_key: ethers_core::types::Bytes,
                pub signature: ethers_core::types::Bytes,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for RegisterValidatorCall {
                #[inline]
                fn clone(&self) -> RegisterValidatorCall {
                    RegisterValidatorCall {
                        staking_key: ::core::clone::Clone::clone(&self.staking_key),
                        consensus_key: ::core::clone::Clone::clone(&self.consensus_key),
                        signature: ::core::clone::Clone::clone(&self.signature),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for RegisterValidatorCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field3_finish(
                        f,
                        "RegisterValidatorCall",
                        "staking_key",
                        &&self.staking_key,
                        "consensus_key",
                        &&self.consensus_key,
                        "signature",
                        &&self.signature,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for RegisterValidatorCall {
                #[inline]
                fn default() -> RegisterValidatorCall {
                    RegisterValidatorCall {
                        staking_key: ::core::default::Default::default(),
                        consensus_key: ::core::default::Default::default(),
                        signature: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for RegisterValidatorCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for RegisterValidatorCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Bytes>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Bytes>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Bytes>;
                }
            }
            impl ::core::marker::StructuralPartialEq for RegisterValidatorCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for RegisterValidatorCall {
                #[inline]
                fn eq(&self, other: &RegisterValidatorCall) -> bool {
                    self.staking_key == other.staking_key
                        && self.consensus_key == other.consensus_key
                        && self.signature == other.signature
                }
            }
            impl ethers_core::abi::AbiType for RegisterValidatorCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Bytes as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Bytes as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Bytes as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for RegisterValidatorCall {}
            impl ethers_core::abi::Tokenizable for RegisterValidatorCall
            where
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 3usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&3usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            staking_key: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            consensus_key: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            signature: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.staking_key.into_token(),
                                self.consensus_key.into_token(),
                                self.signature.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for RegisterValidatorCall
            where
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for RegisterValidatorCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "registerValidator".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [234, 104, 79, 119]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "registerValidator(bytes,bytes,bytes)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for RegisterValidatorCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::Bytes,
                        ethers_core::abi::ParamType::Bytes,
                        ethers_core::abi::ParamType::Bytes,
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for RegisterValidatorCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for RegisterValidatorCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.staking_key),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.consensus_key),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.signature),
                                ),
                            ],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `s_deposit` function with signature `s_deposit(address)` and selector `[169, 155, 221, 28]`
            #[ethcall(name = "s_deposit", abi = "s_deposit(address)")]
            pub struct SDepositCall(pub ethers_core::types::Address);
            #[automatically_derived]
            impl ::core::clone::Clone for SDepositCall {
                #[inline]
                fn clone(&self) -> SDepositCall {
                    SDepositCall(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SDepositCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "SDepositCall",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SDepositCall {
                #[inline]
                fn default() -> SDepositCall {
                    SDepositCall(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for SDepositCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SDepositCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SDepositCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SDepositCall {
                #[inline]
                fn eq(&self, other: &SDepositCall) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for SDepositCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SDepositCall {}
            impl ethers_core::abi::Tokenizable for SDepositCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SDepositCall
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for SDepositCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "s_deposit".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [169, 155, 221, 28]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "s_deposit(address)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for SDepositCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [ethers_core::abi::ParamType::Address];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for SDepositCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for SDepositCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.0)],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `unregisterValidator` function with signature `unregisterValidator(bytes,bytes)` and selector `[92, 100, 153, 92]`
            #[ethcall(
                name = "unregisterValidator",
                abi = "unregisterValidator(bytes,bytes)"
            )]
            pub struct UnregisterValidatorCall {
                pub staking_key: ethers_core::types::Bytes,
                pub signature: ethers_core::types::Bytes,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for UnregisterValidatorCall {
                #[inline]
                fn clone(&self) -> UnregisterValidatorCall {
                    UnregisterValidatorCall {
                        staking_key: ::core::clone::Clone::clone(&self.staking_key),
                        signature: ::core::clone::Clone::clone(&self.signature),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for UnregisterValidatorCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "UnregisterValidatorCall",
                        "staking_key",
                        &&self.staking_key,
                        "signature",
                        &&self.signature,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for UnregisterValidatorCall {
                #[inline]
                fn default() -> UnregisterValidatorCall {
                    UnregisterValidatorCall {
                        staking_key: ::core::default::Default::default(),
                        signature: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for UnregisterValidatorCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for UnregisterValidatorCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Bytes>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Bytes>;
                }
            }
            impl ::core::marker::StructuralPartialEq for UnregisterValidatorCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for UnregisterValidatorCall {
                #[inline]
                fn eq(&self, other: &UnregisterValidatorCall) -> bool {
                    self.staking_key == other.staking_key
                        && self.signature == other.signature
                }
            }
            impl ethers_core::abi::AbiType for UnregisterValidatorCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Bytes as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Bytes as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for UnregisterValidatorCall {}
            impl ethers_core::abi::Tokenizable for UnregisterValidatorCall
            where
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            staking_key: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            signature: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.staking_key.into_token(),
                                self.signature.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for UnregisterValidatorCall
            where
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for UnregisterValidatorCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "unregisterValidator".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [92, 100, 153, 92]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "unregisterValidator(bytes,bytes)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for UnregisterValidatorCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::Bytes,
                        ethers_core::abi::ParamType::Bytes,
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for UnregisterValidatorCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for UnregisterValidatorCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.staking_key),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.signature),
                                ),
                            ],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `withdraw` function with signature `withdraw(uint256)` and selector `[46, 26, 125, 77]`
            #[ethcall(name = "withdraw", abi = "withdraw(uint256)")]
            pub struct WithdrawCall {
                pub amount: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for WithdrawCall {
                #[inline]
                fn clone(&self) -> WithdrawCall {
                    WithdrawCall {
                        amount: ::core::clone::Clone::clone(&self.amount),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for WithdrawCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field1_finish(
                        f,
                        "WithdrawCall",
                        "amount",
                        &&self.amount,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for WithdrawCall {
                #[inline]
                fn default() -> WithdrawCall {
                    WithdrawCall {
                        amount: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for WithdrawCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for WithdrawCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for WithdrawCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for WithdrawCall {
                #[inline]
                fn eq(&self, other: &WithdrawCall) -> bool {
                    self.amount == other.amount
                }
            }
            impl ethers_core::abi::AbiType for WithdrawCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for WithdrawCall {}
            impl ethers_core::abi::Tokenizable for WithdrawCall
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.amount.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for WithdrawCall
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for WithdrawCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "withdraw".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [46, 26, 125, 77]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "withdraw(uint256)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for WithdrawCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [ethers_core::abi::ParamType::Uint(256usize)];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for WithdrawCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for WithdrawCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.amount)],
                        ),
                    )?;
                    Ok(())
                }
            }
            pub enum ValidatorSetCalls {
                Lockup(LockupCall),
                MinStake(MinStakeCall),
                TokenAddress(TokenAddressCall),
                Delegate(DelegateCall),
                Deposit(DepositCall),
                RegisterValidator(RegisterValidatorCall),
                SDeposit(SDepositCall),
                UnregisterValidator(UnregisterValidatorCall),
                Withdraw(WithdrawCall),
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for ValidatorSetCalls {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        ValidatorSetCalls::Lockup(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Lockup",
                                &__self_0,
                            )
                        }
                        ValidatorSetCalls::MinStake(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "MinStake",
                                &__self_0,
                            )
                        }
                        ValidatorSetCalls::TokenAddress(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "TokenAddress",
                                &__self_0,
                            )
                        }
                        ValidatorSetCalls::Delegate(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Delegate",
                                &__self_0,
                            )
                        }
                        ValidatorSetCalls::Deposit(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Deposit",
                                &__self_0,
                            )
                        }
                        ValidatorSetCalls::RegisterValidator(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "RegisterValidator",
                                &__self_0,
                            )
                        }
                        ValidatorSetCalls::SDeposit(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "SDeposit",
                                &__self_0,
                            )
                        }
                        ValidatorSetCalls::UnregisterValidator(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "UnregisterValidator",
                                &__self_0,
                            )
                        }
                        ValidatorSetCalls::Withdraw(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "Withdraw",
                                &__self_0,
                            )
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::clone::Clone for ValidatorSetCalls {
                #[inline]
                fn clone(&self) -> ValidatorSetCalls {
                    match self {
                        ValidatorSetCalls::Lockup(__self_0) => {
                            ValidatorSetCalls::Lockup(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetCalls::MinStake(__self_0) => {
                            ValidatorSetCalls::MinStake(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetCalls::TokenAddress(__self_0) => {
                            ValidatorSetCalls::TokenAddress(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetCalls::Delegate(__self_0) => {
                            ValidatorSetCalls::Delegate(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetCalls::Deposit(__self_0) => {
                            ValidatorSetCalls::Deposit(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetCalls::RegisterValidator(__self_0) => {
                            ValidatorSetCalls::RegisterValidator(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetCalls::SDeposit(__self_0) => {
                            ValidatorSetCalls::SDeposit(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetCalls::UnregisterValidator(__self_0) => {
                            ValidatorSetCalls::UnregisterValidator(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        ValidatorSetCalls::Withdraw(__self_0) => {
                            ValidatorSetCalls::Withdraw(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                    }
                }
            }
            impl ::core::marker::StructuralPartialEq for ValidatorSetCalls {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for ValidatorSetCalls {
                #[inline]
                fn eq(&self, other: &ValidatorSetCalls) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                        && match (self, other) {
                            (
                                ValidatorSetCalls::Lockup(__self_0),
                                ValidatorSetCalls::Lockup(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetCalls::MinStake(__self_0),
                                ValidatorSetCalls::MinStake(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetCalls::TokenAddress(__self_0),
                                ValidatorSetCalls::TokenAddress(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetCalls::Delegate(__self_0),
                                ValidatorSetCalls::Delegate(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetCalls::Deposit(__self_0),
                                ValidatorSetCalls::Deposit(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetCalls::RegisterValidator(__self_0),
                                ValidatorSetCalls::RegisterValidator(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetCalls::SDeposit(__self_0),
                                ValidatorSetCalls::SDeposit(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetCalls::UnregisterValidator(__self_0),
                                ValidatorSetCalls::UnregisterValidator(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                ValidatorSetCalls::Withdraw(__self_0),
                                ValidatorSetCalls::Withdraw(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
            impl ::core::marker::StructuralEq for ValidatorSetCalls {}
            #[automatically_derived]
            impl ::core::cmp::Eq for ValidatorSetCalls {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<LockupCall>;
                    let _: ::core::cmp::AssertParamIsEq<MinStakeCall>;
                    let _: ::core::cmp::AssertParamIsEq<TokenAddressCall>;
                    let _: ::core::cmp::AssertParamIsEq<DelegateCall>;
                    let _: ::core::cmp::AssertParamIsEq<DepositCall>;
                    let _: ::core::cmp::AssertParamIsEq<RegisterValidatorCall>;
                    let _: ::core::cmp::AssertParamIsEq<SDepositCall>;
                    let _: ::core::cmp::AssertParamIsEq<UnregisterValidatorCall>;
                    let _: ::core::cmp::AssertParamIsEq<WithdrawCall>;
                }
            }
            impl ethers_core::abi::Tokenizable for ValidatorSetCalls {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let Ok(decoded) = LockupCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::Lockup(decoded));
                    }
                    if let Ok(decoded) = MinStakeCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::MinStake(decoded));
                    }
                    if let Ok(decoded) = TokenAddressCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::TokenAddress(decoded));
                    }
                    if let Ok(decoded) = DelegateCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::Delegate(decoded));
                    }
                    if let Ok(decoded) = DepositCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::Deposit(decoded));
                    }
                    if let Ok(decoded)
                        = RegisterValidatorCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::RegisterValidator(decoded));
                    }
                    if let Ok(decoded) = SDepositCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::SDeposit(decoded));
                    }
                    if let Ok(decoded)
                        = UnregisterValidatorCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::UnregisterValidator(decoded));
                    }
                    if let Ok(decoded) = WithdrawCall::from_token(token.clone()) {
                        return Ok(ValidatorSetCalls::Withdraw(decoded));
                    }
                    Err(
                        ethers_core::abi::InvalidOutputType(
                            "Failed to decode all type variants".to_string(),
                        ),
                    )
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    match self {
                        ValidatorSetCalls::Lockup(element) => element.into_token(),
                        ValidatorSetCalls::MinStake(element) => element.into_token(),
                        ValidatorSetCalls::TokenAddress(element) => element.into_token(),
                        ValidatorSetCalls::Delegate(element) => element.into_token(),
                        ValidatorSetCalls::Deposit(element) => element.into_token(),
                        ValidatorSetCalls::RegisterValidator(element) => {
                            element.into_token()
                        }
                        ValidatorSetCalls::SDeposit(element) => element.into_token(),
                        ValidatorSetCalls::UnregisterValidator(element) => {
                            element.into_token()
                        }
                        ValidatorSetCalls::Withdraw(element) => element.into_token(),
                    }
                }
            }
            impl ethers_core::abi::TokenizableItem for ValidatorSetCalls {}
            impl ethers_core::abi::AbiDecode for ValidatorSetCalls {
                fn decode(
                    data: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let Ok(decoded)
                        = <LockupCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::Lockup(decoded));
                    }
                    if let Ok(decoded)
                        = <MinStakeCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::MinStake(decoded));
                    }
                    if let Ok(decoded)
                        = <TokenAddressCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::TokenAddress(decoded));
                    }
                    if let Ok(decoded)
                        = <DelegateCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::Delegate(decoded));
                    }
                    if let Ok(decoded)
                        = <DepositCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::Deposit(decoded));
                    }
                    if let Ok(decoded)
                        = <RegisterValidatorCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::RegisterValidator(decoded));
                    }
                    if let Ok(decoded)
                        = <SDepositCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::SDeposit(decoded));
                    }
                    if let Ok(decoded)
                        = <UnregisterValidatorCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::UnregisterValidator(decoded));
                    }
                    if let Ok(decoded)
                        = <WithdrawCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(ValidatorSetCalls::Withdraw(decoded));
                    }
                    Err(ethers_core::abi::Error::InvalidData.into())
                }
            }
            impl ethers_core::abi::AbiEncode for ValidatorSetCalls {
                fn encode(self) -> Vec<u8> {
                    match self {
                        ValidatorSetCalls::Lockup(element) => element.encode(),
                        ValidatorSetCalls::MinStake(element) => element.encode(),
                        ValidatorSetCalls::TokenAddress(element) => element.encode(),
                        ValidatorSetCalls::Delegate(element) => element.encode(),
                        ValidatorSetCalls::Deposit(element) => element.encode(),
                        ValidatorSetCalls::RegisterValidator(element) => element.encode(),
                        ValidatorSetCalls::SDeposit(element) => element.encode(),
                        ValidatorSetCalls::UnregisterValidator(element) => {
                            element.encode()
                        }
                        ValidatorSetCalls::Withdraw(element) => element.encode(),
                    }
                }
            }
            impl ::std::fmt::Display for ValidatorSetCalls {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    match self {
                        ValidatorSetCalls::Lockup(element) => element.fmt(f),
                        ValidatorSetCalls::MinStake(element) => element.fmt(f),
                        ValidatorSetCalls::TokenAddress(element) => element.fmt(f),
                        ValidatorSetCalls::Delegate(element) => element.fmt(f),
                        ValidatorSetCalls::Deposit(element) => element.fmt(f),
                        ValidatorSetCalls::RegisterValidator(element) => element.fmt(f),
                        ValidatorSetCalls::SDeposit(element) => element.fmt(f),
                        ValidatorSetCalls::UnregisterValidator(element) => element.fmt(f),
                        ValidatorSetCalls::Withdraw(element) => element.fmt(f),
                    }
                }
            }
            impl ::std::convert::From<LockupCall> for ValidatorSetCalls {
                fn from(var: LockupCall) -> Self {
                    ValidatorSetCalls::Lockup(var)
                }
            }
            impl ::std::convert::From<MinStakeCall> for ValidatorSetCalls {
                fn from(var: MinStakeCall) -> Self {
                    ValidatorSetCalls::MinStake(var)
                }
            }
            impl ::std::convert::From<TokenAddressCall> for ValidatorSetCalls {
                fn from(var: TokenAddressCall) -> Self {
                    ValidatorSetCalls::TokenAddress(var)
                }
            }
            impl ::std::convert::From<DelegateCall> for ValidatorSetCalls {
                fn from(var: DelegateCall) -> Self {
                    ValidatorSetCalls::Delegate(var)
                }
            }
            impl ::std::convert::From<DepositCall> for ValidatorSetCalls {
                fn from(var: DepositCall) -> Self {
                    ValidatorSetCalls::Deposit(var)
                }
            }
            impl ::std::convert::From<RegisterValidatorCall> for ValidatorSetCalls {
                fn from(var: RegisterValidatorCall) -> Self {
                    ValidatorSetCalls::RegisterValidator(var)
                }
            }
            impl ::std::convert::From<SDepositCall> for ValidatorSetCalls {
                fn from(var: SDepositCall) -> Self {
                    ValidatorSetCalls::SDeposit(var)
                }
            }
            impl ::std::convert::From<UnregisterValidatorCall> for ValidatorSetCalls {
                fn from(var: UnregisterValidatorCall) -> Self {
                    ValidatorSetCalls::UnregisterValidator(var)
                }
            }
            impl ::std::convert::From<WithdrawCall> for ValidatorSetCalls {
                fn from(var: WithdrawCall) -> Self {
                    ValidatorSetCalls::Withdraw(var)
                }
            }
            ///Container type for all return fields from the `LOCKUP` function with signature `LOCKUP()` and selector `[132, 90, 239, 75]`
            pub struct LockupReturn(pub ethers_core::types::U256);
            #[automatically_derived]
            impl ::core::clone::Clone for LockupReturn {
                #[inline]
                fn clone(&self) -> LockupReturn {
                    LockupReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for LockupReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "LockupReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for LockupReturn {
                #[inline]
                fn default() -> LockupReturn {
                    LockupReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for LockupReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for LockupReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for LockupReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for LockupReturn {
                #[inline]
                fn eq(&self, other: &LockupReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for LockupReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for LockupReturn {}
            impl ethers_core::abi::Tokenizable for LockupReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for LockupReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for LockupReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for LockupReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `MIN_STAKE` function with signature `MIN_STAKE()` and selector `[203, 28, 43, 92]`
            pub struct MinStakeReturn(pub ethers_core::types::U256);
            #[automatically_derived]
            impl ::core::clone::Clone for MinStakeReturn {
                #[inline]
                fn clone(&self) -> MinStakeReturn {
                    MinStakeReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MinStakeReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "MinStakeReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for MinStakeReturn {
                #[inline]
                fn default() -> MinStakeReturn {
                    MinStakeReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for MinStakeReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MinStakeReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for MinStakeReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MinStakeReturn {
                #[inline]
                fn eq(&self, other: &MinStakeReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for MinStakeReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for MinStakeReturn {}
            impl ethers_core::abi::Tokenizable for MinStakeReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for MinStakeReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for MinStakeReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for MinStakeReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `TOKEN_ADDRESS` function with signature `TOKEN_ADDRESS()` and selector `[11, 223, 83, 0]`
            pub struct TokenAddressReturn(pub ethers_core::types::Address);
            #[automatically_derived]
            impl ::core::clone::Clone for TokenAddressReturn {
                #[inline]
                fn clone(&self) -> TokenAddressReturn {
                    TokenAddressReturn(::core::clone::Clone::clone(&self.0))
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for TokenAddressReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "TokenAddressReturn",
                        &&self.0,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for TokenAddressReturn {
                #[inline]
                fn default() -> TokenAddressReturn {
                    TokenAddressReturn(::core::default::Default::default())
                }
            }
            impl ::core::marker::StructuralEq for TokenAddressReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for TokenAddressReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Address>;
                }
            }
            impl ::core::marker::StructuralPartialEq for TokenAddressReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for TokenAddressReturn {
                #[inline]
                fn eq(&self, other: &TokenAddressReturn) -> bool {
                    self.0 == other.0
                }
            }
            impl ethers_core::abi::AbiType for TokenAddressReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::Address as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for TokenAddressReturn {}
            impl ethers_core::abi::Tokenizable for TokenAddressReturn
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(
                            Self(
                                ethers_core::abi::Tokenizable::from_token(
                                    iter.next().unwrap(),
                                )?,
                            ),
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.0.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for TokenAddressReturn
            where
                ethers_core::types::Address: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for TokenAddressReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for TokenAddressReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
            ///Container type for all return fields from the `s_deposit` function with signature `s_deposit(address)` and selector `[169, 155, 221, 28]`
            pub struct SDepositReturn {
                pub amount: ethers_core::types::U256,
                pub lockup: ethers_core::types::U256,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for SDepositReturn {
                #[inline]
                fn clone(&self) -> SDepositReturn {
                    SDepositReturn {
                        amount: ::core::clone::Clone::clone(&self.amount),
                        lockup: ::core::clone::Clone::clone(&self.lockup),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SDepositReturn {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "SDepositReturn",
                        "amount",
                        &&self.amount,
                        "lockup",
                        &&self.lockup,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SDepositReturn {
                #[inline]
                fn default() -> SDepositReturn {
                    SDepositReturn {
                        amount: ::core::default::Default::default(),
                        lockup: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for SDepositReturn {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SDepositReturn {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::U256>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SDepositReturn {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SDepositReturn {
                #[inline]
                fn eq(&self, other: &SDepositReturn) -> bool {
                    self.amount == other.amount && self.lockup == other.lockup
                }
            }
            impl ethers_core::abi::AbiType for SDepositReturn {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::U256 as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SDepositReturn {}
            impl ethers_core::abi::Tokenizable for SDepositReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            lockup: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.amount.into_token(),
                                self.lockup.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SDepositReturn
            where
                ethers_core::types::U256: ethers_core::abi::Tokenize,
                ethers_core::types::U256: ethers_core::abi::Tokenize,
            {}
            impl ethers_core::abi::AbiDecode for SDepositReturn {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let ethers_core::abi::ParamType::Tuple(params)
                        = <Self as ethers_core::abi::AbiType>::param_type() {
                        let tokens = ethers_core::abi::decode(&params, bytes.as_ref())?;
                        Ok(
                            <Self as ethers_core::abi::Tokenizable>::from_token(
                                ethers_core::abi::Token::Tuple(tokens),
                            )?,
                        )
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType(
                                    "Expected tuple".to_string(),
                                )
                                .into(),
                        )
                    }
                }
            }
            impl ethers_core::abi::AbiEncode for SDepositReturn {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    ethers_core::abi::encode(&tokens)
                }
            }
        }
    }
    pub mod bridge {
        pub use message::*;
        #[allow(clippy::too_many_arguments, non_camel_case_types)]
        pub mod message {
            #![allow(clippy::enum_variant_names)]
            #![allow(dead_code)]
            #![allow(clippy::type_complexity)]
            #![allow(unused_imports)]
            ///Message was auto-generated with ethers-rs Abigen. More information at: https://github.com/gakonst/ethers-rs
            use std::sync::Arc;
            use ethers_core::{
                abi::{Abi, Token, Detokenize, InvalidOutputType, Tokenizable},
                types::*,
            };
            use ethers_contract::{
                Contract, builders::{ContractCall, Event},
                Lazy,
            };
            use ethers_providers::Middleware;
            pub static MESSAGE_ABI: ethers_contract::Lazy<ethers_core::abi::Abi> = ethers_contract::Lazy::new(||
            {
                ethers_core::utils::__serde_json::from_str(
                        "[{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"sender\",\"type\":\"bytes32\",\"components\":[],\"indexed\":true},{\"internalType\":\"bytes32\",\"name\":\"recipient\",\"type\":\"bytes32\",\"components\":[],\"indexed\":true},{\"internalType\":\"bytes32\",\"name\":\"owner\",\"type\":\"bytes32\",\"components\":[],\"indexed\":false},{\"internalType\":\"uint64\",\"name\":\"nonce\",\"type\":\"uint64\",\"components\":[],\"indexed\":false},{\"internalType\":\"uint64\",\"name\":\"amount\",\"type\":\"uint64\",\"components\":[],\"indexed\":false},{\"internalType\":\"bytes\",\"name\":\"data\",\"type\":\"bytes\",\"components\":[],\"indexed\":false}],\"type\":\"event\",\"name\":\"SentMessage\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"recipient\",\"type\":\"bytes32\",\"components\":[]}],\"stateMutability\":\"payable\",\"type\":\"function\",\"name\":\"sendETH\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"recipient\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"data\",\"type\":\"bytes\",\"components\":[]}],\"stateMutability\":\"payable\",\"type\":\"function\",\"name\":\"sendMessage\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"recipient\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"owner\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"data\",\"type\":\"bytes\",\"components\":[]}],\"stateMutability\":\"payable\",\"type\":\"function\",\"name\":\"sendMessageWithOwner\",\"outputs\":[]}]",
                    )
                    .expect("invalid abi")
            });
            pub struct Message<M>(ethers_contract::Contract<M>);
            impl<M> Clone for Message<M> {
                fn clone(&self) -> Self {
                    Message(self.0.clone())
                }
            }
            impl<M> std::ops::Deref for Message<M> {
                type Target = ethers_contract::Contract<M>;
                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }
            impl<M: ethers_providers::Middleware> std::fmt::Debug for Message<M> {
                fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.debug_tuple("Message").field(&self.address()).finish()
                }
            }
            impl<M: ethers_providers::Middleware> Message<M> {
                /// Creates a new contract instance with the specified `ethers`
                /// client at the given `Address`. The contract derefs to a `ethers::Contract`
                /// object
                pub fn new<T: Into<ethers_core::types::Address>>(
                    address: T,
                    client: ::std::sync::Arc<M>,
                ) -> Self {
                    ethers_contract::Contract::new(
                            address.into(),
                            MESSAGE_ABI.clone(),
                            client,
                        )
                        .into()
                }
                ///Calls the contract's `sendETH` (0x0846691f) function
                pub fn send_eth(
                    &self,
                    recipient: [u8; 32],
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash([8, 70, 105, 31], recipient)
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `sendMessage` (0x23c640e7) function
                pub fn send_message(
                    &self,
                    recipient: [u8; 32],
                    data: ethers_core::types::Bytes,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash([35, 198, 64, 231], (recipient, data))
                        .expect("method not found (this should never happen)")
                }
                ///Calls the contract's `sendMessageWithOwner` (0x143516fd) function
                pub fn send_message_with_owner(
                    &self,
                    recipient: [u8; 32],
                    owner: [u8; 32],
                    data: ethers_core::types::Bytes,
                ) -> ethers_contract::builders::ContractCall<M, ()> {
                    self.0
                        .method_hash([20, 53, 22, 253], (recipient, owner, data))
                        .expect("method not found (this should never happen)")
                }
                ///Gets the contract's `SentMessage` event
                pub fn sent_message_filter(
                    &self,
                ) -> ethers_contract::builders::Event<M, SentMessageFilter> {
                    self.0.event()
                }
                /// Returns an [`Event`](#ethers_contract::builders::Event) builder for all events of this contract
                pub fn events(
                    &self,
                ) -> ethers_contract::builders::Event<M, SentMessageFilter> {
                    self.0.event_with_filter(Default::default())
                }
            }
            impl<M: ethers_providers::Middleware> From<ethers_contract::Contract<M>>
            for Message<M> {
                fn from(contract: ethers_contract::Contract<M>) -> Self {
                    Self(contract)
                }
            }
            #[ethevent(
                name = "SentMessage",
                abi = "SentMessage(bytes32,bytes32,bytes32,uint64,uint64,bytes)"
            )]
            pub struct SentMessageFilter {
                #[ethevent(indexed)]
                pub sender: [u8; 32],
                #[ethevent(indexed)]
                pub recipient: [u8; 32],
                pub owner: [u8; 32],
                pub nonce: u64,
                pub amount: u64,
                pub data: ethers_core::types::Bytes,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for SentMessageFilter {
                #[inline]
                fn clone(&self) -> SentMessageFilter {
                    SentMessageFilter {
                        sender: ::core::clone::Clone::clone(&self.sender),
                        recipient: ::core::clone::Clone::clone(&self.recipient),
                        owner: ::core::clone::Clone::clone(&self.owner),
                        nonce: ::core::clone::Clone::clone(&self.nonce),
                        amount: ::core::clone::Clone::clone(&self.amount),
                        data: ::core::clone::Clone::clone(&self.data),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SentMessageFilter {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    let names: &'static _ = &[
                        "sender",
                        "recipient",
                        "owner",
                        "nonce",
                        "amount",
                        "data",
                    ];
                    let values: &[&dyn ::core::fmt::Debug] = &[
                        &&self.sender,
                        &&self.recipient,
                        &&self.owner,
                        &&self.nonce,
                        &&self.amount,
                        &&self.data,
                    ];
                    ::core::fmt::Formatter::debug_struct_fields_finish(
                        f,
                        "SentMessageFilter",
                        names,
                        values,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SentMessageFilter {
                #[inline]
                fn default() -> SentMessageFilter {
                    SentMessageFilter {
                        sender: ::core::default::Default::default(),
                        recipient: ::core::default::Default::default(),
                        owner: ::core::default::Default::default(),
                        nonce: ::core::default::Default::default(),
                        amount: ::core::default::Default::default(),
                        data: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for SentMessageFilter {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SentMessageFilter {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<u64>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Bytes>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SentMessageFilter {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SentMessageFilter {
                #[inline]
                fn eq(&self, other: &SentMessageFilter) -> bool {
                    self.sender == other.sender && self.recipient == other.recipient
                        && self.owner == other.owner && self.nonce == other.nonce
                        && self.amount == other.amount && self.data == other.data
                }
            }
            impl ethers_core::abi::AbiType for SentMessageFilter {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <u64 as ethers_core::abi::AbiType>::param_type(),
                                <u64 as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Bytes as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SentMessageFilter {}
            impl ethers_core::abi::Tokenizable for SentMessageFilter
            where
                [u8; 32]: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                u64: ethers_core::abi::Tokenize,
                u64: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 6usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&6usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            sender: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            recipient: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            owner: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            nonce: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            amount: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            data: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.sender.into_token(),
                                self.recipient.into_token(),
                                self.owner.into_token(),
                                self.nonce.into_token(),
                                self.amount.into_token(),
                                self.data.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SentMessageFilter
            where
                [u8; 32]: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                u64: ethers_core::abi::Tokenize,
                u64: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthEvent for SentMessageFilter {
                fn name() -> ::std::borrow::Cow<'static, str> {
                    "SentMessage".into()
                }
                fn signature() -> ethers_core::types::H256 {
                    ethers_core::types::H256([
                        110,
                        119,
                        124,
                        52,
                        149,
                        16,
                        53,
                        86,
                        5,
                        145,
                        250,
                        195,
                        0,
                        81,
                        89,
                        66,
                        130,
                        28,
                        202,
                        19,
                        154,
                        184,
                        165,
                        20,
                        235,
                        17,
                        113,
                        41,
                        4,
                        142,
                        33,
                        178,
                    ])
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "SentMessage(bytes32,bytes32,bytes32,uint64,uint64,bytes)".into()
                }
                fn decode_log(
                    log: &ethers_core::abi::RawLog,
                ) -> Result<Self, ethers_core::abi::Error>
                where
                    Self: Sized,
                {
                    let ethers_core::abi::RawLog { data, topics } = log;
                    let event_signature = topics
                        .get(0)
                        .ok_or(ethers_core::abi::Error::InvalidData)?;
                    if event_signature != &Self::signature() {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let topic_types = <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            ethers_core::abi::ParamType::FixedBytes(32usize),
                            ethers_core::abi::ParamType::FixedBytes(32usize),
                        ]),
                    );
                    let data_types = [
                        ethers_core::abi::ParamType::FixedBytes(32usize),
                        ethers_core::abi::ParamType::Uint(64usize),
                        ethers_core::abi::ParamType::Uint(64usize),
                        ethers_core::abi::ParamType::Bytes,
                    ];
                    let flat_topics = topics
                        .iter()
                        .skip(1)
                        .flat_map(|t| t.as_ref().to_vec())
                        .collect::<Vec<u8>>();
                    let topic_tokens = ethers_core::abi::decode(
                        &topic_types,
                        &flat_topics,
                    )?;
                    if topic_tokens.len() != topics.len() - 1 {
                        return Err(ethers_core::abi::Error::InvalidData);
                    }
                    let data_tokens = ethers_core::abi::decode(&data_types, data)?;
                    let tokens: Vec<_> = topic_tokens
                        .into_iter()
                        .chain(data_tokens.into_iter())
                        .collect();
                    ethers_core::abi::Tokenizable::from_token(
                            ethers_core::abi::Token::Tuple(tokens),
                        )
                        .map_err(|_| ethers_core::abi::Error::InvalidData)
                }
                fn is_anonymous() -> bool {
                    false
                }
            }
            impl ::std::fmt::Display for SentMessageFilter {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.sender[..]),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.recipient[..]),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.owner[..]),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.nonce)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &[""],
                            &[::core::fmt::ArgumentV1::new_debug(&self.amount)],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.data),
                                ),
                            ],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `sendETH` function with signature `sendETH(bytes32)` and selector `[8, 70, 105, 31]`
            #[ethcall(name = "sendETH", abi = "sendETH(bytes32)")]
            pub struct SendETHCall {
                pub recipient: [u8; 32],
            }
            #[automatically_derived]
            impl ::core::clone::Clone for SendETHCall {
                #[inline]
                fn clone(&self) -> SendETHCall {
                    SendETHCall {
                        recipient: ::core::clone::Clone::clone(&self.recipient),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SendETHCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field1_finish(
                        f,
                        "SendETHCall",
                        "recipient",
                        &&self.recipient,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SendETHCall {
                #[inline]
                fn default() -> SendETHCall {
                    SendETHCall {
                        recipient: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for SendETHCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SendETHCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SendETHCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SendETHCall {
                #[inline]
                fn eq(&self, other: &SendETHCall) -> bool {
                    self.recipient == other.recipient
                }
            }
            impl ethers_core::abi::AbiType for SendETHCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SendETHCall {}
            impl ethers_core::abi::Tokenizable for SendETHCall
            where
                [u8; 32]: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 1usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&1usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            recipient: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([self.recipient.into_token()]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SendETHCall
            where
                [u8; 32]: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for SendETHCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "sendETH".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [8, 70, 105, 31]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "sendETH(bytes32)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for SendETHCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [ethers_core::abi::ParamType::FixedBytes(32usize)];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for SendETHCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for SendETHCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.recipient[..]),
                                ),
                            ],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `sendMessage` function with signature `sendMessage(bytes32,bytes)` and selector `[35, 198, 64, 231]`
            #[ethcall(name = "sendMessage", abi = "sendMessage(bytes32,bytes)")]
            pub struct SendMessageCall {
                pub recipient: [u8; 32],
                pub data: ethers_core::types::Bytes,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for SendMessageCall {
                #[inline]
                fn clone(&self) -> SendMessageCall {
                    SendMessageCall {
                        recipient: ::core::clone::Clone::clone(&self.recipient),
                        data: ::core::clone::Clone::clone(&self.data),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SendMessageCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "SendMessageCall",
                        "recipient",
                        &&self.recipient,
                        "data",
                        &&self.data,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SendMessageCall {
                #[inline]
                fn default() -> SendMessageCall {
                    SendMessageCall {
                        recipient: ::core::default::Default::default(),
                        data: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for SendMessageCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SendMessageCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Bytes>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SendMessageCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SendMessageCall {
                #[inline]
                fn eq(&self, other: &SendMessageCall) -> bool {
                    self.recipient == other.recipient && self.data == other.data
                }
            }
            impl ethers_core::abi::AbiType for SendMessageCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Bytes as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SendMessageCall {}
            impl ethers_core::abi::Tokenizable for SendMessageCall
            where
                [u8; 32]: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 2usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&2usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            recipient: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            data: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.recipient.into_token(),
                                self.data.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SendMessageCall
            where
                [u8; 32]: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for SendMessageCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "sendMessage".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [35, 198, 64, 231]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "sendMessage(bytes32,bytes)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for SendMessageCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::FixedBytes(32usize),
                        ethers_core::abi::ParamType::Bytes,
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for SendMessageCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for SendMessageCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.recipient[..]),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.data),
                                ),
                            ],
                        ),
                    )?;
                    Ok(())
                }
            }
            ///Container type for all input parameters for the `sendMessageWithOwner` function with signature `sendMessageWithOwner(bytes32,bytes32,bytes)` and selector `[20, 53, 22, 253]`
            #[ethcall(
                name = "sendMessageWithOwner",
                abi = "sendMessageWithOwner(bytes32,bytes32,bytes)"
            )]
            pub struct SendMessageWithOwnerCall {
                pub recipient: [u8; 32],
                pub owner: [u8; 32],
                pub data: ethers_core::types::Bytes,
            }
            #[automatically_derived]
            impl ::core::clone::Clone for SendMessageWithOwnerCall {
                #[inline]
                fn clone(&self) -> SendMessageWithOwnerCall {
                    SendMessageWithOwnerCall {
                        recipient: ::core::clone::Clone::clone(&self.recipient),
                        owner: ::core::clone::Clone::clone(&self.owner),
                        data: ::core::clone::Clone::clone(&self.data),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for SendMessageWithOwnerCall {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field3_finish(
                        f,
                        "SendMessageWithOwnerCall",
                        "recipient",
                        &&self.recipient,
                        "owner",
                        &&self.owner,
                        "data",
                        &&self.data,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for SendMessageWithOwnerCall {
                #[inline]
                fn default() -> SendMessageWithOwnerCall {
                    SendMessageWithOwnerCall {
                        recipient: ::core::default::Default::default(),
                        owner: ::core::default::Default::default(),
                        data: ::core::default::Default::default(),
                    }
                }
            }
            impl ::core::marker::StructuralEq for SendMessageWithOwnerCall {}
            #[automatically_derived]
            impl ::core::cmp::Eq for SendMessageWithOwnerCall {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<[u8; 32]>;
                    let _: ::core::cmp::AssertParamIsEq<ethers_core::types::Bytes>;
                }
            }
            impl ::core::marker::StructuralPartialEq for SendMessageWithOwnerCall {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for SendMessageWithOwnerCall {
                #[inline]
                fn eq(&self, other: &SendMessageWithOwnerCall) -> bool {
                    self.recipient == other.recipient && self.owner == other.owner
                        && self.data == other.data
                }
            }
            impl ethers_core::abi::AbiType for SendMessageWithOwnerCall {
                fn param_type() -> ethers_core::abi::ParamType {
                    ethers_core::abi::ParamType::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <[u8; 32] as ethers_core::abi::AbiType>::param_type(),
                                <ethers_core::types::Bytes as ethers_core::abi::AbiType>::param_type(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::AbiArrayType for SendMessageWithOwnerCall {}
            impl ethers_core::abi::Tokenizable for SendMessageWithOwnerCall
            where
                [u8; 32]: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let ethers_core::abi::Token::Tuple(tokens) = token {
                        if tokens.len() != 3usize {
                            return Err(
                                ethers_core::abi::InvalidOutputType({
                                    let res = ::alloc::fmt::format(
                                        ::core::fmt::Arguments::new_v1(
                                            &["Expected ", " tokens, got ", ": "],
                                            &[
                                                ::core::fmt::ArgumentV1::new_display(&3usize),
                                                ::core::fmt::ArgumentV1::new_display(&tokens.len()),
                                                ::core::fmt::ArgumentV1::new_debug(&tokens),
                                            ],
                                        ),
                                    );
                                    res
                                }),
                            );
                        }
                        let mut iter = tokens.into_iter();
                        Ok(Self {
                            recipient: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            owner: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                            data: ethers_core::abi::Tokenizable::from_token(
                                iter.next().unwrap(),
                            )?,
                        })
                    } else {
                        Err(
                            ethers_core::abi::InvalidOutputType({
                                let res = ::alloc::fmt::format(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Expected Tuple, got "],
                                        &[::core::fmt::ArgumentV1::new_debug(&token)],
                                    ),
                                );
                                res
                            }),
                        )
                    }
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    ethers_core::abi::Token::Tuple(
                        <[_]>::into_vec(
                            #[rustc_box]
                            ::alloc::boxed::Box::new([
                                self.recipient.into_token(),
                                self.owner.into_token(),
                                self.data.into_token(),
                            ]),
                        ),
                    )
                }
            }
            impl ethers_core::abi::TokenizableItem for SendMessageWithOwnerCall
            where
                [u8; 32]: ethers_core::abi::Tokenize,
                [u8; 32]: ethers_core::abi::Tokenize,
                ethers_core::types::Bytes: ethers_core::abi::Tokenize,
            {}
            impl ethers_contract::EthCall for SendMessageWithOwnerCall {
                fn function_name() -> ::std::borrow::Cow<'static, str> {
                    "sendMessageWithOwner".into()
                }
                fn selector() -> ethers_core::types::Selector {
                    [20, 53, 22, 253]
                }
                fn abi_signature() -> ::std::borrow::Cow<'static, str> {
                    "sendMessageWithOwner(bytes32,bytes32,bytes)".into()
                }
            }
            impl ethers_core::abi::AbiDecode for SendMessageWithOwnerCall {
                fn decode(
                    bytes: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    let bytes = bytes.as_ref();
                    if bytes.len() < 4
                        || bytes[..4] != <Self as ethers_contract::EthCall>::selector()
                    {
                        return Err(ethers_contract::AbiError::WrongSelector);
                    }
                    let data_types = [
                        ethers_core::abi::ParamType::FixedBytes(32usize),
                        ethers_core::abi::ParamType::FixedBytes(32usize),
                        ethers_core::abi::ParamType::Bytes,
                    ];
                    let data_tokens = ethers_core::abi::decode(
                        &data_types,
                        &bytes[4..],
                    )?;
                    Ok(
                        <Self as ethers_core::abi::Tokenizable>::from_token(
                            ethers_core::abi::Token::Tuple(data_tokens),
                        )?,
                    )
                }
            }
            impl ethers_core::abi::AbiEncode for SendMessageWithOwnerCall {
                fn encode(self) -> ::std::vec::Vec<u8> {
                    let tokens = ethers_core::abi::Tokenize::into_tokens(self);
                    let selector = <Self as ethers_contract::EthCall>::selector();
                    let encoded = ethers_core::abi::encode(&tokens);
                    selector.iter().copied().chain(encoded.into_iter()).collect()
                }
            }
            impl ::std::fmt::Display for SendMessageWithOwnerCall {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.recipient[..]),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.owner[..]),
                                ),
                            ],
                        ),
                    )?;
                    f.write_fmt(::core::fmt::Arguments::new_v1(&[", "], &[]))?;
                    f.write_fmt(
                        ::core::fmt::Arguments::new_v1(
                            &["0x"],
                            &[
                                ::core::fmt::ArgumentV1::new_display(
                                    &ethers_core::utils::hex::encode(&self.data),
                                ),
                            ],
                        ),
                    )?;
                    Ok(())
                }
            }
            pub enum MessageCalls {
                SendETH(SendETHCall),
                SendMessage(SendMessageCall),
                SendMessageWithOwner(SendMessageWithOwnerCall),
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for MessageCalls {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match self {
                        MessageCalls::SendETH(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "SendETH",
                                &__self_0,
                            )
                        }
                        MessageCalls::SendMessage(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "SendMessage",
                                &__self_0,
                            )
                        }
                        MessageCalls::SendMessageWithOwner(__self_0) => {
                            ::core::fmt::Formatter::debug_tuple_field1_finish(
                                f,
                                "SendMessageWithOwner",
                                &__self_0,
                            )
                        }
                    }
                }
            }
            #[automatically_derived]
            impl ::core::clone::Clone for MessageCalls {
                #[inline]
                fn clone(&self) -> MessageCalls {
                    match self {
                        MessageCalls::SendETH(__self_0) => {
                            MessageCalls::SendETH(::core::clone::Clone::clone(__self_0))
                        }
                        MessageCalls::SendMessage(__self_0) => {
                            MessageCalls::SendMessage(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                        MessageCalls::SendMessageWithOwner(__self_0) => {
                            MessageCalls::SendMessageWithOwner(
                                ::core::clone::Clone::clone(__self_0),
                            )
                        }
                    }
                }
            }
            impl ::core::marker::StructuralPartialEq for MessageCalls {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for MessageCalls {
                #[inline]
                fn eq(&self, other: &MessageCalls) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                        && match (self, other) {
                            (
                                MessageCalls::SendETH(__self_0),
                                MessageCalls::SendETH(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                MessageCalls::SendMessage(__self_0),
                                MessageCalls::SendMessage(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            (
                                MessageCalls::SendMessageWithOwner(__self_0),
                                MessageCalls::SendMessageWithOwner(__arg1_0),
                            ) => *__self_0 == *__arg1_0,
                            _ => unsafe { ::core::intrinsics::unreachable() }
                        }
                }
            }
            impl ::core::marker::StructuralEq for MessageCalls {}
            #[automatically_derived]
            impl ::core::cmp::Eq for MessageCalls {
                #[inline]
                #[doc(hidden)]
                #[no_coverage]
                fn assert_receiver_is_total_eq(&self) -> () {
                    let _: ::core::cmp::AssertParamIsEq<SendETHCall>;
                    let _: ::core::cmp::AssertParamIsEq<SendMessageCall>;
                    let _: ::core::cmp::AssertParamIsEq<SendMessageWithOwnerCall>;
                }
            }
            impl ethers_core::abi::Tokenizable for MessageCalls {
                fn from_token(
                    token: ethers_core::abi::Token,
                ) -> Result<Self, ethers_core::abi::InvalidOutputType>
                where
                    Self: Sized,
                {
                    if let Ok(decoded) = SendETHCall::from_token(token.clone()) {
                        return Ok(MessageCalls::SendETH(decoded));
                    }
                    if let Ok(decoded) = SendMessageCall::from_token(token.clone()) {
                        return Ok(MessageCalls::SendMessage(decoded));
                    }
                    if let Ok(decoded)
                        = SendMessageWithOwnerCall::from_token(token.clone()) {
                        return Ok(MessageCalls::SendMessageWithOwner(decoded));
                    }
                    Err(
                        ethers_core::abi::InvalidOutputType(
                            "Failed to decode all type variants".to_string(),
                        ),
                    )
                }
                fn into_token(self) -> ethers_core::abi::Token {
                    match self {
                        MessageCalls::SendETH(element) => element.into_token(),
                        MessageCalls::SendMessage(element) => element.into_token(),
                        MessageCalls::SendMessageWithOwner(element) => {
                            element.into_token()
                        }
                    }
                }
            }
            impl ethers_core::abi::TokenizableItem for MessageCalls {}
            impl ethers_core::abi::AbiDecode for MessageCalls {
                fn decode(
                    data: impl AsRef<[u8]>,
                ) -> Result<Self, ethers_core::abi::AbiError> {
                    if let Ok(decoded)
                        = <SendETHCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(MessageCalls::SendETH(decoded));
                    }
                    if let Ok(decoded)
                        = <SendMessageCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(MessageCalls::SendMessage(decoded));
                    }
                    if let Ok(decoded)
                        = <SendMessageWithOwnerCall as ethers_core::abi::AbiDecode>::decode(
                            data.as_ref(),
                        ) {
                        return Ok(MessageCalls::SendMessageWithOwner(decoded));
                    }
                    Err(ethers_core::abi::Error::InvalidData.into())
                }
            }
            impl ethers_core::abi::AbiEncode for MessageCalls {
                fn encode(self) -> Vec<u8> {
                    match self {
                        MessageCalls::SendETH(element) => element.encode(),
                        MessageCalls::SendMessage(element) => element.encode(),
                        MessageCalls::SendMessageWithOwner(element) => element.encode(),
                    }
                }
            }
            impl ::std::fmt::Display for MessageCalls {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    match self {
                        MessageCalls::SendETH(element) => element.fmt(f),
                        MessageCalls::SendMessage(element) => element.fmt(f),
                        MessageCalls::SendMessageWithOwner(element) => element.fmt(f),
                    }
                }
            }
            impl ::std::convert::From<SendETHCall> for MessageCalls {
                fn from(var: SendETHCall) -> Self {
                    MessageCalls::SendETH(var)
                }
            }
            impl ::std::convert::From<SendMessageCall> for MessageCalls {
                fn from(var: SendMessageCall) -> Self {
                    MessageCalls::SendMessage(var)
                }
            }
            impl ::std::convert::From<SendMessageWithOwnerCall> for MessageCalls {
                fn from(var: SendMessageWithOwnerCall) -> Self {
                    MessageCalls::SendMessageWithOwner(var)
                }
            }
        }
    }
    pub use bridge::Message;
    pub use fuel::Fuel;
    pub use validators::ValidatorSet;
}
pub(crate) mod config {
    use ethers_core::types::{H160, H256};
    use fuel_core_interfaces::model::DaBlockHeight;
    use once_cell::sync::Lazy;
    use sha3::{Digest, Keccak256};
    use std::{str::FromStr, time::Duration};
    pub(crate) const REPORT_INIT_SYNC_PROGRESS_EVERY_N_BLOCKS: DaBlockHeight = 1000;
    pub(crate) const NUMBER_OF_TRIES_FOR_INITIAL_SYNC: u64 = 10;
    pub fn keccak256(data: &'static str) -> H256 {
        let out = Keccak256::digest(data.as_bytes());
        H256::from_slice(out.as_slice())
    }
    pub(crate) static ETH_LOG_MESSAGE: Lazy<H256> = Lazy::new(|| keccak256(
        "SentMessage(bytes32,bytes32,bytes32,uint64,uint64,bytes)",
    ));
    pub(crate) static ETH_FUEL_BLOCK_COMMITTED: Lazy<H256> = Lazy::new(|| keccak256(
        "BlockCommitted(bytes32,uint32)",
    ));
    pub struct Config {
        /// Number of da block after which messages/stakes/validators become finalized.
        pub da_finalization: DaBlockHeight,
        /// Uri address to ethereum client.
        pub eth_client: Option<String>,
        /// Ethereum chain_id.
        pub eth_chain_id: u64,
        /// Contract to publish commit fuel block.
        pub eth_v2_commit_contract: Option<H160>,
        /// Ethereum contract address. Create EthAddress into fuel_types.
        pub eth_v2_listening_contracts: Vec<H160>,
        /// Block number after we can start filtering events related to fuel.
        /// It does not need to be accurate and can be set in past before contracts are deployed.
        pub eth_v2_contracts_deployment: DaBlockHeight,
        /// Number of blocks that will be asked at one time from client, used for initial sync.
        pub initial_sync_step: usize,
        /// Refresh rate of waiting for eth client to finish its initial sync.
        pub initial_sync_refresh: Duration,
    }
    #[automatically_derived]
    impl ::core::clone::Clone for Config {
        #[inline]
        fn clone(&self) -> Config {
            Config {
                da_finalization: ::core::clone::Clone::clone(&self.da_finalization),
                eth_client: ::core::clone::Clone::clone(&self.eth_client),
                eth_chain_id: ::core::clone::Clone::clone(&self.eth_chain_id),
                eth_v2_commit_contract: ::core::clone::Clone::clone(
                    &self.eth_v2_commit_contract,
                ),
                eth_v2_listening_contracts: ::core::clone::Clone::clone(
                    &self.eth_v2_listening_contracts,
                ),
                eth_v2_contracts_deployment: ::core::clone::Clone::clone(
                    &self.eth_v2_contracts_deployment,
                ),
                initial_sync_step: ::core::clone::Clone::clone(&self.initial_sync_step),
                initial_sync_refresh: ::core::clone::Clone::clone(
                    &self.initial_sync_refresh,
                ),
            }
        }
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for Config {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "da_finalization",
                "eth_client",
                "eth_chain_id",
                "eth_v2_commit_contract",
                "eth_v2_listening_contracts",
                "eth_v2_contracts_deployment",
                "initial_sync_step",
                "initial_sync_refresh",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &&self.da_finalization,
                &&self.eth_client,
                &&self.eth_chain_id,
                &&self.eth_v2_commit_contract,
                &&self.eth_v2_listening_contracts,
                &&self.eth_v2_contracts_deployment,
                &&self.initial_sync_step,
                &&self.initial_sync_refresh,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "Config",
                names,
                values,
            )
        }
    }
    impl Default for Config {
        fn default() -> Self {
            Self {
                da_finalization: 100,
                eth_client: None,
                eth_chain_id: 1,
                eth_v2_commit_contract: Some(
                    H160::from_str("0x03E4538018285e1c03CCce2F92C9538c87606911").unwrap(),
                ),
                eth_v2_listening_contracts: <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        H160::from_str("0x03E4538018285e1c03CCce2F92C9538c87606911")
                            .unwrap(),
                    ]),
                ),
                eth_v2_contracts_deployment: 0,
                initial_sync_step: 1000,
                initial_sync_refresh: Duration::from_secs(5),
            }
        }
    }
    impl Config {
        pub fn eth_v2_contracts_deployment(&self) -> DaBlockHeight {
            self.eth_v2_contracts_deployment
        }
        pub fn eth_v2_listening_contracts(&self) -> &[H160] {
            &self.eth_v2_listening_contracts
        }
        pub fn da_finalization(&self) -> DaBlockHeight {
            self.da_finalization
        }
        pub fn eth_client(&self) -> Option<&str> {
            self.eth_client.as_deref()
        }
        pub fn initial_sync_step(&self) -> usize {
            self.initial_sync_step
        }
        pub fn initial_sync_refresh(&self) -> Duration {
            self.initial_sync_refresh
        }
        pub fn eth_chain_id(&self) -> u64 {
            self.eth_chain_id
        }
        pub fn eth_v2_commit_contract(&self) -> Option<H160> {
            self.eth_v2_commit_contract
        }
    }
}
pub(crate) mod log {
    use std::convert::TryInto;
    use crate::{abi, config};
    use anyhow::anyhow;
    use ethers_contract::EthEvent;
    use ethers_core::{abi::RawLog, types::Log};
    use fuel_core_interfaces::{
        common::fuel_types::{Address, Bytes32, Word},
        model::{DaBlockHeight, Message},
    };
    /// Bridge message send from da to fuel network.
    pub struct MessageLog {
        pub sender: Address,
        pub recipient: Address,
        pub owner: Address,
        pub nonce: Word,
        pub amount: Word,
        pub data: Vec<u8>,
        pub da_height: DaBlockHeight,
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for MessageLog {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let names: &'static _ = &[
                "sender",
                "recipient",
                "owner",
                "nonce",
                "amount",
                "data",
                "da_height",
            ];
            let values: &[&dyn ::core::fmt::Debug] = &[
                &&self.sender,
                &&self.recipient,
                &&self.owner,
                &&self.nonce,
                &&self.amount,
                &&self.data,
                &&self.da_height,
            ];
            ::core::fmt::Formatter::debug_struct_fields_finish(
                f,
                "MessageLog",
                names,
                values,
            )
        }
    }
    #[automatically_derived]
    impl ::core::clone::Clone for MessageLog {
        #[inline]
        fn clone(&self) -> MessageLog {
            MessageLog {
                sender: ::core::clone::Clone::clone(&self.sender),
                recipient: ::core::clone::Clone::clone(&self.recipient),
                owner: ::core::clone::Clone::clone(&self.owner),
                nonce: ::core::clone::Clone::clone(&self.nonce),
                amount: ::core::clone::Clone::clone(&self.amount),
                data: ::core::clone::Clone::clone(&self.data),
                da_height: ::core::clone::Clone::clone(&self.da_height),
            }
        }
    }
    impl ::core::marker::StructuralEq for MessageLog {}
    #[automatically_derived]
    impl ::core::cmp::Eq for MessageLog {
        #[inline]
        #[doc(hidden)]
        #[no_coverage]
        fn assert_receiver_is_total_eq(&self) -> () {
            let _: ::core::cmp::AssertParamIsEq<Address>;
            let _: ::core::cmp::AssertParamIsEq<Word>;
            let _: ::core::cmp::AssertParamIsEq<Vec<u8>>;
            let _: ::core::cmp::AssertParamIsEq<DaBlockHeight>;
        }
    }
    impl ::core::marker::StructuralPartialEq for MessageLog {}
    #[automatically_derived]
    impl ::core::cmp::PartialEq for MessageLog {
        #[inline]
        fn eq(&self, other: &MessageLog) -> bool {
            self.sender == other.sender && self.recipient == other.recipient
                && self.owner == other.owner && self.nonce == other.nonce
                && self.amount == other.amount && self.data == other.data
                && self.da_height == other.da_height
        }
    }
    impl From<&MessageLog> for Message {
        fn from(message: &MessageLog) -> Self {
            Self {
                sender: message.sender,
                recipient: message.recipient,
                nonce: message.nonce,
                amount: message.amount,
                data: message.data.clone(),
                da_height: message.da_height,
                fuel_block_spend: None,
            }
        }
    }
    pub enum EthEventLog {
        Message(MessageLog),
        FuelBlockCommitted { block_root: Bytes32, height: Word },
        Ignored,
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for EthEventLog {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match self {
                EthEventLog::Message(__self_0) => {
                    ::core::fmt::Formatter::debug_tuple_field1_finish(
                        f,
                        "Message",
                        &__self_0,
                    )
                }
                EthEventLog::FuelBlockCommitted {
                    block_root: __self_0,
                    height: __self_1,
                } => {
                    ::core::fmt::Formatter::debug_struct_field2_finish(
                        f,
                        "FuelBlockCommitted",
                        "block_root",
                        &__self_0,
                        "height",
                        &__self_1,
                    )
                }
                EthEventLog::Ignored => ::core::fmt::Formatter::write_str(f, "Ignored"),
            }
        }
    }
    #[automatically_derived]
    impl ::core::clone::Clone for EthEventLog {
        #[inline]
        fn clone(&self) -> EthEventLog {
            match self {
                EthEventLog::Message(__self_0) => {
                    EthEventLog::Message(::core::clone::Clone::clone(__self_0))
                }
                EthEventLog::FuelBlockCommitted {
                    block_root: __self_0,
                    height: __self_1,
                } => {
                    EthEventLog::FuelBlockCommitted {
                        block_root: ::core::clone::Clone::clone(__self_0),
                        height: ::core::clone::Clone::clone(__self_1),
                    }
                }
                EthEventLog::Ignored => EthEventLog::Ignored,
            }
        }
    }
    impl ::core::marker::StructuralEq for EthEventLog {}
    #[automatically_derived]
    impl ::core::cmp::Eq for EthEventLog {
        #[inline]
        #[doc(hidden)]
        #[no_coverage]
        fn assert_receiver_is_total_eq(&self) -> () {
            let _: ::core::cmp::AssertParamIsEq<MessageLog>;
            let _: ::core::cmp::AssertParamIsEq<Bytes32>;
            let _: ::core::cmp::AssertParamIsEq<Word>;
        }
    }
    impl ::core::marker::StructuralPartialEq for EthEventLog {}
    #[automatically_derived]
    impl ::core::cmp::PartialEq for EthEventLog {
        #[inline]
        fn eq(&self, other: &EthEventLog) -> bool {
            let __self_tag = ::core::intrinsics::discriminant_value(self);
            let __arg1_tag = ::core::intrinsics::discriminant_value(other);
            __self_tag == __arg1_tag
                && match (self, other) {
                    (EthEventLog::Message(__self_0), EthEventLog::Message(__arg1_0)) => {
                        *__self_0 == *__arg1_0
                    }
                    (
                        EthEventLog::FuelBlockCommitted {
                            block_root: __self_0,
                            height: __self_1,
                        },
                        EthEventLog::FuelBlockCommitted {
                            block_root: __arg1_0,
                            height: __arg1_1,
                        },
                    ) => *__self_0 == *__arg1_0 && *__self_1 == *__arg1_1,
                    _ => true,
                }
        }
    }
    impl TryFrom<&Log> for EthEventLog {
        type Error = anyhow::Error;
        fn try_from(log: &Log) -> Result<Self, Self::Error> {
            if log.topics.is_empty() {
                return Err(
                    ::anyhow::__private::must_use({
                        let error = ::anyhow::__private::format_err(
                            ::core::fmt::Arguments::new_v1(&["Topic list is empty"], &[]),
                        );
                        error
                    }),
                );
            }
            let log = match log.topics[0] {
                n if n == *config::ETH_LOG_MESSAGE => {
                    if log.topics.len() != 3 {
                        return Err(
                            ::anyhow::__private::must_use({
                                let error = ::anyhow::__private::format_err(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Malformed topics for Message"],
                                        &[],
                                    ),
                                );
                                error
                            }),
                        );
                    }
                    let raw_log = RawLog {
                        topics: log.topics.clone(),
                        data: log.data.to_vec(),
                    };
                    let message = abi::bridge::SentMessageFilter::decode_log(&raw_log)?;
                    let amount = message.amount;
                    let data = message.data.to_vec();
                    let nonce = message.nonce;
                    let owner = Address::from(message.owner);
                    let recipient = Address::from(message.recipient);
                    let sender = Address::from(message.sender);
                    Self::Message(MessageLog {
                        amount,
                        data,
                        nonce,
                        sender,
                        recipient,
                        owner,
                        da_height: log.block_number.unwrap().as_u64(),
                    })
                }
                n if n == *config::ETH_FUEL_BLOCK_COMMITTED => {
                    if log.topics.len() != 3 {
                        return Err(
                            ::anyhow::__private::must_use({
                                let error = ::anyhow::__private::format_err(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Malformed topics for FuelBlockCommitted"],
                                        &[],
                                    ),
                                );
                                error
                            }),
                        );
                    }
                    let block_root: [u8; 32] = log
                        .topics[1]
                        .try_into()
                        .map_err(|_| {
                            ::anyhow::__private::must_use({
                                let error = ::anyhow::__private::format_err(
                                    ::core::fmt::Arguments::new_v1(
                                        &["Malformed block root topic for FuelBlockCommitted"],
                                        &[],
                                    ),
                                );
                                error
                            })
                        })?;
                    let block_root = Bytes32::new(block_root);
                    let height = <[u8; 4]>::try_from(&log.topics[2][28..])
                        .map(u32::from_be_bytes)
                        .expect("Slice bounds are predefined") as u64;
                    Self::FuelBlockCommitted {
                        block_root,
                        height,
                    }
                }
                _ => Self::Ignored,
            };
            Ok(log)
        }
    }
}
mod relayer {
    use crate::{log::EthEventLog, Config};
    use anyhow::Result;
    use ethers_core::types::{Filter, Log, ValueOrArray, H160};
    use ethers_providers::{Http, Middleware, Provider, ProviderError};
    use fuel_core_interfaces::{model::Message, relayer::RelayerDb};
    use std::{convert::TryInto, ops::Deref, sync::Arc};
    use synced::update_synced;
    use tokio::sync::watch;
    mod state {
        use core::ops::RangeInclusive;
        pub use state_builder::*;
        use std::ops::Deref;
        mod state_builder {
            //! Type safe state building
            use super::*;
            pub struct EthRemote;
            pub struct EthRemoteCurrent(u64);
            pub struct EthRemoteFinalizationPeriod(u64);
            pub struct EthRemoteHeights(EthHeights);
            pub struct EthLocal;
            pub struct EthLocalFinalized(u64);
            pub struct FuelLocal;
            pub struct FuelLocalCurrent(u32);
            pub struct FuelLocalFinalized(u32);
            impl EthRemote {
                pub fn current(height: u64) -> EthRemoteCurrent {
                    EthRemoteCurrent(height)
                }
                pub fn finalization_period(p: u64) -> EthRemoteFinalizationPeriod {
                    EthRemoteFinalizationPeriod(p)
                }
            }
            impl EthRemoteCurrent {
                pub fn finalization_period(self, p: u64) -> EthRemoteHeights {
                    EthRemoteHeights(EthHeights::new(self.0, p))
                }
            }
            impl EthRemoteFinalizationPeriod {
                pub fn current(self, height: u64) -> EthRemoteHeights {
                    EthRemoteHeights(EthHeights::new(height, self.0))
                }
            }
            impl EthLocal {
                pub fn finalized(f: u64) -> EthLocalFinalized {
                    EthLocalFinalized(f)
                }
            }
            impl EthRemoteHeights {
                pub fn with_local(self, f: EthLocalFinalized) -> EthState {
                    EthState {
                        remote: self.0,
                        local: f.0,
                    }
                }
            }
            impl EthLocalFinalized {
                pub fn with_remote(self, remote: EthRemoteHeights) -> EthState {
                    EthState {
                        remote: remote.0,
                        local: self.0,
                    }
                }
            }
            impl FuelLocal {
                pub fn current(height: u32) -> FuelLocalCurrent {
                    FuelLocalCurrent(height)
                }
                pub fn finalized(height: u32) -> FuelLocalFinalized {
                    FuelLocalFinalized(height)
                }
            }
            impl FuelLocalCurrent {
                pub fn finalized(self, height: u32) -> FuelState {
                    FuelState {
                        local: FuelHeights::new(self.0, height),
                    }
                }
            }
            impl FuelLocalFinalized {
                pub fn current(self, height: u32) -> FuelState {
                    FuelState {
                        local: FuelHeights::new(height, self.0),
                    }
                }
            }
            impl FuelState {
                pub fn with_eth(self, eth: EthState) -> SyncState {
                    SyncState { eth, fuel: self }
                }
            }
            impl EthState {
                pub fn with_fuel(self, fuel: FuelState) -> SyncState {
                    SyncState { eth: self, fuel }
                }
            }
        }
        pub struct SyncState {
            eth: EthState,
            fuel: FuelState,
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for SyncState {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field2_finish(
                    f,
                    "SyncState",
                    "eth",
                    &&self.eth,
                    "fuel",
                    &&self.fuel,
                )
            }
        }
        pub struct EthState {
            remote: EthHeights,
            local: Height,
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for EthState {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field2_finish(
                    f,
                    "EthState",
                    "remote",
                    &&self.remote,
                    "local",
                    &&self.local,
                )
            }
        }
        pub struct FuelState {
            local: FuelHeights,
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for FuelState {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_struct_field1_finish(
                    f,
                    "FuelState",
                    "local",
                    &&self.local,
                )
            }
        }
        type Height = u64;
        struct Heights<T>(RangeInclusive<T>);
        #[automatically_derived]
        impl<T: ::core::clone::Clone> ::core::clone::Clone for Heights<T> {
            #[inline]
            fn clone(&self) -> Heights<T> {
                Heights(::core::clone::Clone::clone(&self.0))
            }
        }
        #[automatically_derived]
        impl<T: ::core::fmt::Debug> ::core::fmt::Debug for Heights<T> {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_tuple_field1_finish(f, "Heights", &&self.0)
            }
        }
        struct EthHeights(Heights<u64>);
        #[automatically_derived]
        impl ::core::fmt::Debug for EthHeights {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_tuple_field1_finish(
                    f,
                    "EthHeights",
                    &&self.0,
                )
            }
        }
        struct FuelHeights(Heights<u32>);
        #[automatically_derived]
        impl ::core::fmt::Debug for FuelHeights {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_tuple_field1_finish(
                    f,
                    "FuelHeights",
                    &&self.0,
                )
            }
        }
        pub struct EthSyncGap(Heights<u64>);
        #[automatically_derived]
        impl ::core::clone::Clone for EthSyncGap {
            #[inline]
            fn clone(&self) -> EthSyncGap {
                EthSyncGap(::core::clone::Clone::clone(&self.0))
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for EthSyncGap {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::debug_tuple_field1_finish(
                    f,
                    "EthSyncGap",
                    &&self.0,
                )
            }
        }
        impl SyncState {
            pub fn is_synced(&self) -> bool {
                self.eth.is_synced() && self.fuel.is_synced()
            }
            pub fn needs_to_sync_eth(&self) -> Option<EthSyncGap> {
                self.eth.needs_to_sync_eth()
            }
            pub fn needs_to_publish_fuel(&self) -> Option<u32> {
                self.fuel.needs_to_publish()
            }
        }
        impl EthState {
            fn is_synced(&self) -> bool {
                self.local >= self.remote.finalized()
            }
            pub fn needs_to_sync_eth(&self) -> Option<EthSyncGap> {
                (!self.is_synced())
                    .then(|| EthSyncGap::new(self.local, self.remote.finalized()))
            }
        }
        impl FuelState {
            fn is_synced(&self) -> bool {
                self.local.finalized() >= self.local.current()
            }
            pub fn needs_to_publish(&self) -> Option<u32> {
                (!self.is_synced()).then(|| self.local.current())
            }
        }
        impl EthHeights {
            fn new(current: u64, finalization_period: u64) -> Self {
                Self(Heights(current.saturating_sub(finalization_period)..=current))
            }
        }
        impl EthSyncGap {
            fn new(local: u64, remote: u64) -> Self {
                Self(Heights(local..=remote))
            }
            pub fn oldest(&self) -> u64 {
                *self.0.0.start()
            }
            pub fn latest(&self) -> u64 {
                *self.0.0.end()
            }
        }
        impl FuelHeights {
            fn new(current: u32, finalized: u32) -> Self {
                Self(Heights(finalized..=current))
            }
        }
        impl<T: Copy> Heights<T> {
            fn current(&self) -> T {
                *self.0.end()
            }
            fn finalized(&self) -> T {
                *self.0.start()
            }
        }
        impl Deref for EthHeights {
            type Target = Heights<u64>;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
        impl Deref for FuelHeights {
            type Target = Heights<u32>;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    }
    mod synced {
        use tokio::sync::watch;
        use super::state::*;
        pub fn update_synced(synced: &watch::Sender<bool>, state: &SyncState) {
            update_synced_inner(synced, state.is_synced())
        }
        /// Updates the sender state but only notifies if the
        /// state has become synced.
        fn update_synced_inner(synced: &watch::Sender<bool>, is_synced: bool) {
            synced
                .send_if_modified(|last_state| {
                    *last_state = is_synced;
                    is_synced
                });
        }
    }
    type Synced = watch::Receiver<bool>;
    type Database = Box<dyn RelayerDb>;
    pub struct RelayerHandle {
        synced: Synced,
    }
    pub struct Relayer<P>
    where
        P: Middleware<Error = ProviderError>,
    {
        synced: watch::Sender<bool>,
        eth_node: Arc<P>,
        database: Database,
        config: Config,
    }
    impl<P> Relayer<P>
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        fn new(
            synced: watch::Sender<bool>,
            eth_node: P,
            database: Database,
            config: Config,
        ) -> Self {
            Self {
                synced,
                eth_node: Arc::new(eth_node),
                database,
                config,
            }
        }
        async fn download_logs(
            &self,
            eth_sync_gap: &state::EthSyncGap,
        ) -> Result<Vec<Log>, ProviderError> {
            download_logs(
                    eth_sync_gap,
                    self.config.eth_v2_listening_contracts.clone(),
                    &self.eth_node,
                )
                .await
        }
    }
    impl RelayerHandle {
        pub fn start(database: Database, config: Config) -> Self {
            let eth_node = ::core::panicking::panic("not yet implemented");
            Self::start_inner::<Provider<Http>>(eth_node, database, config)
        }
        fn start_inner<P>(eth_node: P, database: Database, config: Config) -> Self
        where
            P: Middleware<Error = ProviderError> + 'static,
        {
            let (tx, rx) = watch::channel(false);
            let synced = rx;
            let r = Self { synced: synced.clone() };
            run(Relayer::new(tx, eth_node, database, config));
            r
        }
        pub async fn await_synced(&self) -> Result<()> {
            let mut rx = self.synced.clone();
            if !rx.borrow_and_update().deref() {
                rx.changed().await?;
            }
            Ok(())
        }
    }
    fn run<P>(mut relayer: Relayer<P>)
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        let jh = tokio::task::spawn(async move {
            loop {
                let fuel_state = state::FuelLocal::current(
                        relayer.database.get_chain_height().await.into(),
                    )
                    .finalized(
                        relayer
                            .database
                            .get_last_committed_finalized_fuel_height()
                            .await
                            .into(),
                    );
                let eth_state = state::EthRemote::current(
                        relayer.eth_node.get_block_number().await.unwrap().as_u64(),
                    )
                    .finalization_period(relayer.config.da_finalization)
                    .with_local(
                        state::EthLocal::finalized(
                            relayer.database.get_finalized_da_height().await,
                        ),
                    );
                let state = eth_state.with_fuel(fuel_state);
                if let Some(eth_sync_gap) = state.needs_to_sync_eth() {
                    let logs = relayer.download_logs(&eth_sync_gap).await.unwrap();
                    let events: Vec<EthEventLog> = logs
                        .iter()
                        .map(|l| l.try_into().unwrap())
                        .collect();
                    for event in events {
                        match event {
                            EthEventLog::Message(m) => {
                                let m: Message = (&m).into();
                                relayer.database.insert(&m.id(), &m).unwrap();
                            }
                            EthEventLog::FuelBlockCommitted { block_root, height } => {
                                relayer
                                    .database
                                    .set_last_committed_finalized_fuel_height(height.into())
                                    .await;
                            }
                            EthEventLog::Ignored => {
                                ::core::panicking::panic("not yet implemented")
                            }
                        }
                    }
                    relayer
                        .database
                        .set_finalized_da_height(eth_sync_gap.latest())
                        .await;
                }
                if let Some(new_block_height) = state.needs_to_publish_fuel() {
                    if let Some(contract) = relayer.config.eth_v2_commit_contract.clone()
                    {
                        let client = crate::abi::fuel::Fuel::new(
                            contract,
                            relayer.eth_node.clone(),
                        );
                        client
                            .commit_block(
                                new_block_height,
                                Default::default(),
                                Default::default(),
                                Default::default(),
                                Default::default(),
                                Default::default(),
                                Default::default(),
                                Default::default(),
                            )
                            .call()
                            .await
                            .unwrap();
                    }
                }
                update_synced(&relayer.synced, &state);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        });
    }
    async fn download_logs<P>(
        eth_sync_gap: &state::EthSyncGap,
        contracts: Vec<H160>,
        eth_node: &P,
    ) -> Result<Vec<Log>, ProviderError>
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        let filter = Filter::new()
            .from_block(eth_sync_gap.oldest())
            .to_block(eth_sync_gap.latest())
            .address(ValueOrArray::Array(contracts));
        eth_node.get_logs(&filter).await
    }
}
pub use config::Config;
pub use ethers_core::types::{H160, H256};
pub use relayer::RelayerHandle;
