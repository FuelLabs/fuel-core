//! Compressed versions of fuel-tx types needed for DA storage.

// TODO: remove malleabile fields

use fuel_core_types::{
    fuel_tx::{
        self,
        TxPointer,
    },
    fuel_types::{
        self,
        BlockHeight,
        Bytes32,
        Word,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::registry::{
    tables,
    Key,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Transaction {
    Script(Script),
    Create(Create),
    Mint(Mint),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Script {
    script_gas_limit: Word,
    script: Key<tables::ScriptCode>,
    script_data: Vec<u8>,
    policies: fuel_tx::policies::Policies,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    witnesses: Vec<Key<tables::Witness>>,
    receipts_root: Bytes32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Input {
    CoinSigned {
        utxo_id: TxPointer,
        owner: Key<tables::Address>,
        amount: Word,
        asset_id: Key<tables::AssetId>,
        tx_pointer: TxPointer,
        witness_index: u8,
        maturity: BlockHeight,
    },
    CoinPredicate {
        utxo_id: TxPointer,
        owner: Key<tables::Address>,
        amount: Word,
        asset_id: Key<tables::AssetId>,
        tx_pointer: TxPointer,
        maturity: BlockHeight,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    },
    Contract {
        utxo_id: TxPointer,
        balance_root: Bytes32,
        state_root: Bytes32,
        tx_pointer: TxPointer,
        asset_id: Key<tables::AssetId>,
    },
    MessageCoinSigned {
        sender: Key<tables::Address>,
        recipient: Key<tables::Address>,
        amount: Word,
        nonce: fuel_types::Nonce,
        witness_index: u8,
        data: Vec<u8>,
    },
    MessageCoinPredicate {
        sender: Key<tables::Address>,
        recipient: Key<tables::Address>,
        amount: Word,
        nonce: fuel_types::Nonce,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    },
    MessageDataSigned {
        sender: Key<tables::Address>,
        recipient: Key<tables::Address>,
        amount: Word,
        nonce: fuel_types::Nonce,
        witness_index: u8,
        data: Vec<u8>,
    },
    MessageDataPredicate {
        sender: Key<tables::Address>,
        recipient: Key<tables::Address>,
        amount: Word,
        nonce: fuel_types::Nonce,
        data: Vec<u8>,
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Output {
    Coin {
        to: Key<tables::Address>,
        amount: Word,
        asset_id: Key<tables::AssetId>,
    },

    Contract {
        input_index: u8,
    },

    Change,

    Variable,

    ContractCreated {
        contract_id: TxPointer,
        state_root: Bytes32,
    },
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Create {
    bytecode_length: Word,
    bytecode_witness_index: u8,
    policies: fuel_tx::policies::Policies,
    storage_slots: Vec<fuel_tx::StorageSlot>,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    witnesses: Vec<fuel_tx::Witness>,
    salt: fuel_types::Salt,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Mint {
    tx_pointer: TxPointer,
    input_contract: TxPointer,
    output_contract: OutputContract,
    mint_amount: Word,
    mint_asset_id: Key<tables::AssetId>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct OutputContract {
    input_index: u8,
}

mod compaction {
    // This could be done using a derive macro as well. Not sure if that's worth it.

    use fuel_core_types::fuel_tx::field::{
        Inputs,
        *,
    };

    use crate::{
        compression::CompactionContext,
        registry::{
            db,
            CountPerTable,
        },
        Compactable,
    };

    use super::*;

    impl Compactable for fuel_tx::Transaction {
        type Compact = super::Transaction;

        fn count(&self) -> CountPerTable {
            match self {
                fuel_tx::Transaction::Script(tx) => tx.count(),
                fuel_tx::Transaction::Create(tx) => tx.count(),
                fuel_tx::Transaction::Mint(tx) => tx.count(),
            }
        }

        fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
        where
            R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
        {
            match self {
                fuel_tx::Transaction::Script(tx) => {
                    Self::Compact::Script(tx.compact(ctx))
                }
                fuel_tx::Transaction::Create(tx) => {
                    Self::Compact::Create(tx.compact(ctx))
                }
                fuel_tx::Transaction::Mint(tx) => Self::Compact::Mint(tx.compact(ctx)),
            }
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            match compact {
                super::Transaction::Script(tx) => {
                    fuel_tx::Transaction::Script(fuel_tx::Script::decompact(tx, reg))
                }
                super::Transaction::Create(tx) => {
                    fuel_tx::Transaction::Create(fuel_tx::Create::decompact(tx, reg))
                }
                super::Transaction::Mint(tx) => {
                    fuel_tx::Transaction::Mint(fuel_tx::Mint::decompact(tx, reg))
                }
            }
        }
    }

    impl Compactable for fuel_tx::Script {
        type Compact = super::Script;

        fn count(&self) -> CountPerTable {
            let mut sum = CountPerTable {
                ScriptCode: 1,
                Witness: self.witnesses().len(),
                ..Default::default()
            };
            for input in self.inputs() {
                sum += <fuel_tx::Input as Compactable>::count(input);
            }
            for output in self.outputs() {
                sum += <fuel_tx::Output as Compactable>::count(output);
            }
            sum
        }

        fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
        where
            R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
        {
            Self::Compact {
                script_gas_limit: self.script_gas_limit,
                script: ctx.to_key::<tables::ScriptCode>(self.script().clone()),
                script_data: self.script_data().clone(),
                policies: self.policies().clone(),
                inputs: self.inputs().map(|i| i.compact(ctx)).collect(),
                outputs: self.outputs().map(|o| o.compact(ctx)).collect(),
                witnesses: self.witnesses().map(|w| w.compact(ctx)).collect(),
                receipts_root: self.receipts_root,
            }
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            Self {
                script_gas_limit: compact.script_gas_limit,
                script: reg.read::<tables::ScriptCode>(compact.script),
                script_data: compact.script_data,
                policies: compact.policies,
                inputs: compact.inputs().map(|i| i.decompact(reg)).collect(),
                outputs: compact.outputs().map(|o| o.decompact(reg)).collect(),
                witnesses: compact.witnesses().map(|w| w.decompact(reg)).collect(),
                receipts_root: compact.receipts_root,
            }
        }
    }

    impl Compactable for fuel_tx::Create {
        type Compact = super::Create;

        fn count(&self) -> CountPerTable {
            let sum = CountPerTable {
                Witness: self.witnesses().len(),
                ..Default::default()
            };
            for input in self.inputs() {
                sum += input.count();
            }
            for output in self.outputs() {
                sum += output.count();
            }
            sum
        }

        fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
        where
            R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
        {
            Self::Compact {
                bytecode_length: self.bytecode_length,
                bytecode_witness_index: self.bytecode_witness_index,
                policies: self.policies().clone(),
                storage_slots: self.storage_slots().clone(),
                inputs: self.inputs().map(|i| i.compact(ctx)).collect(),
                outputs: self.outputs().map(|o| o.compact(ctx)).collect(),
                witnesses: self.witnesses().map(|w| w.compact(ctx)).collect(),
                salt: *self.salt(),
            }
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            Self {
                bytecode_length: compact.bytecode_length,
                bytecode_witness_index: compact.bytecode_witness_index,
                policies: compact.policies,
                storage_slots: compact.storage_slots,
                inputs: compact
                    .inputs()
                    .map(|i| fuel_tx::Input::decompact(i, reg))
                    .collect(),
                outputs: compact.outputs().map(|o| o.decompact(reg)).collect(),
                witnesses: compact.witnesses().map(|w| w.decompact(reg)).collect(),
                salt: compact.salt,
            }
        }
    }

    impl Compactable for fuel_tx::Mint {
        type Compact = super::Mint;

        fn count(&self) -> CountPerTable {
            let sum = CountPerTable {
                AssetId: 1,
                Witness: self.witnesses().len(),
                ..Default::default()
            };
            for input in self.inputs() {
                sum += input.count();
            }
            for output in self.outputs() {
                sum += output.count();
            }
            sum
        }

        fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
        where
            R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
        {
            Self::Compact {
                tx_pointer: self.tx_pointer,
                input_contract: self.input_contract.compact(ctx),
                output_contract: self.output_contract.compact(ctx),
                mint_amount: self.mint_amount,
                mint_asset_id: ctx.to_key::<tables::AssetId>(self.mint_asset_id),
            }
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            Self {
                tx_pointer: compact.tx_pointer,
                input_contract: compact.input_contract.decompact(reg),
                output_contract: compact.output_contract.decompact(reg),
                mint_amount: compact.mint_amount,
                mint_asset_id: reg.read::<tables::AssetId>(compact.mint_asset_id),
            }
        }
    }

    impl Compactable for fuel_tx::input::Input {
        type Compact = super::Input;

        fn count(&self) -> CountPerTable {
            match self {
                fuel_tx::input::Input::CoinSigned(_) => CountPerTable {
                    AssetId: 1,
                    Address: 1,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::CoinPredicate(_) => CountPerTable {
                    AssetId: 1,
                    Address: 1,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::Contract(_) => CountPerTable {
                    AssetId: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::MessageCoinSigned(_) => CountPerTable {
                    AssetId: 1,
                    Address: 2,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::MessageCoinPredicate(_) => CountPerTable {
                    AssetId: 1,
                    Address: 2,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::MessageDataSigned(_) => CountPerTable {
                    AssetId: 1,
                    Address: 2,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::MessageDataPredicate(_) => CountPerTable {
                    AssetId: 1,
                    Address: 2,
                    Witness: 1,
                    ..Default::default()
                },
            }
        }

        fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
        where
            R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
        {
            match self {
                fuel_tx::input::Input::CoinSigned(input) => {
                    Self::Compact::CoinSigned(input.compact(ctx))
                }
                fuel_tx::input::Input::CoinPredicate(input) => {
                    Self::Compact::CoinPredicate(input.compact(ctx))
                }
                fuel_tx::input::Input::Contract(input) => {
                    Self::Compact::Contract(input.compact(ctx))
                }
                fuel_tx::input::Input::MessageCoinSigned(input) => {
                    Self::Compact::MessageCoinSigned(input.compact(ctx))
                }
                fuel_tx::input::Input::MessageCoinPredicate(input) => {
                    Self::Compact::MessageCoinPredicate(input.compact(ctx))
                }
                fuel_tx::input::Input::MessageDataSigned(input) => {
                    Self::Compact::MessageDataSigned(input.compact(ctx))
                }
                fuel_tx::input::Input::MessageDataPredicate(input) => {
                    Self::Compact::MessageDataPredicate(input.compact(ctx))
                }
            }
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            todo!()
        }
    }

    impl Compactable for fuel_tx::input::Output {
        type Compact = super::Output;

        fn count(&self) -> CountPerTable {
            match self {
                fuel_tx::input::Input::CoinSigned(_) => CountPerTable {
                    AssetId: 1,
                    Address: 1,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::CoinPredicate(_) => CountPerTable {
                    AssetId: 1,
                    Address: 1,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::Contract(_) => CountPerTable {
                    AssetId: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::MessageCoinSigned(_) => CountPerTable {
                    AssetId: 1,
                    Address: 2,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::MessageCoinPredicate(_) => CountPerTable {
                    AssetId: 1,
                    Address: 2,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::MessageDataSigned(_) => CountPerTable {
                    AssetId: 1,
                    Address: 2,
                    Witness: 1,
                    ..Default::default()
                },
                fuel_tx::input::Input::MessageDataPredicate(_) => CountPerTable {
                    AssetId: 1,
                    Address: 2,
                    Witness: 1,
                    ..Default::default()
                },
            }
        }

        fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
        where
            R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex,
        {
            match self {
                fuel_tx::input::Input::CoinSigned(input) => {
                    Self::Compact::CoinSigned(input.compact(ctx))
                }
                fuel_tx::input::Input::CoinPredicate(input) => {
                    Self::Compact::CoinPredicate(input.compact(ctx))
                }
                fuel_tx::input::Input::Contract(input) => {
                    Self::Compact::Contract(input.compact(ctx))
                }
                fuel_tx::input::Input::MessageCoinSigned(input) => {
                    Self::Compact::MessageCoinSigned(input.compact(ctx))
                }
                fuel_tx::input::Input::MessageCoinPredicate(input) => {
                    Self::Compact::MessageCoinPredicate(input.compact(ctx))
                }
                fuel_tx::input::Input::MessageDataSigned(input) => {
                    Self::Compact::MessageDataSigned(input.compact(ctx))
                }
                fuel_tx::input::Input::MessageDataPredicate(input) => {
                    Self::Compact::MessageDataPredicate(input.compact(ctx))
                }
            }
        }

        fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
        where
            R: db::RegistryRead,
        {
            todo!()
        }
    }
}
