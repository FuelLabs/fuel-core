use crate::types::ContractId;
use fuel_core_types::{
    fuel_tx::{
        Address,
        UtxoId,
    },
    fuel_types::Nonce,
};
use std::{
    collections::HashSet,
    time::Duration,
};

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct BlackList {
    /// Blacklisted addresses.
    pub(crate) owners: HashSet<Address>,
    /// Blacklisted UTXO ids.
    pub(crate) coins: HashSet<UtxoId>,
    /// Blacklisted messages by `Nonce`.
    pub(crate) messages: HashSet<Nonce>,
    /// Blacklisted contracts.
    pub(crate) contracts: HashSet<ContractId>,
}

impl BlackList {
    pub fn new(
        owners: Vec<Address>,
        utxo_ids: Vec<UtxoId>,
        messages: Vec<Nonce>,
        contracts: Vec<ContractId>,
    ) -> Self {
        Self {
            owners: owners.into_iter().collect(),
            coins: utxo_ids.into_iter().collect(),
            messages: messages.into_iter().collect(),
            contracts: contracts.into_iter().collect(),
        }
    }

    pub fn contains_address(&self, address: &Address) -> bool {
        self.owners.contains(address)
    }

    pub fn contains_coin(&self, utxo_id: &UtxoId) -> bool {
        self.coins.contains(utxo_id)
    }

    pub fn contains_message(&self, nonce: &Nonce) -> bool {
        self.messages.contains(nonce)
    }

    pub fn contains_contract(&self, contract_id: &ContractId) -> bool {
        self.contracts.contains(contract_id)
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum number of transactions inside the pool
    pub max_tx: usize,
    /// max depth of connected UTXO excluding contracts
    pub max_depth: usize,
    /// Flag to disable utxo existence and signature checks
    pub utxo_validation: bool,
    /// Enables prometheus metrics for this fuel-service
    pub metrics: bool,
    /// Transaction TTL
    pub transaction_ttl: Duration,
    /// The number of allowed active transaction status subscriptions.
    pub number_of_active_subscription: usize,
    /// The blacklist used to validate transaction.
    pub blacklist: BlackList,
}

#[cfg(feature = "test-helpers")]
impl Default for Config {
    fn default() -> Self {
        let max_tx = 4064;
        let max_depth = 10;
        let utxo_validation = true;
        let metrics = false;
        // 5 minute TTL
        let transaction_ttl = Duration::from_secs(60 * 5);
        let number_of_active_subscription = max_tx;
        Self::new(
            max_tx,
            max_depth,
            utxo_validation,
            metrics,
            transaction_ttl,
            number_of_active_subscription,
            Default::default(),
        )
    }
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_tx: usize,
        max_depth: usize,
        utxo_validation: bool,
        metrics: bool,
        transaction_ttl: Duration,
        number_of_active_subscription: usize,
        blacklist: BlackList,
    ) -> Self {
        // # Dev-note: If you add a new field, be sure that this field is propagated correctly
        //  in all places where `new` is used.
        Self {
            max_tx,
            max_depth,
            utxo_validation,
            metrics,
            transaction_ttl,
            number_of_active_subscription,
            blacklist,
        }
    }
}
