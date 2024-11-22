use crate::ports::Signer;
use anyhow::anyhow;
use cosmrs::AccountId;

/// Configuration for the shared sequencer client
#[derive(Debug, Clone)]
pub struct Config {
    /// Whether the sequencer is enabled.
    pub enabled: bool,
    /// Address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:26657")
    pub tendermint_api: String,
    /// Address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:1317")
    pub blockchain_api: String,
    /// Coin denominator for the sequencer fee payment
    /// (e.g. "utest")
    pub coin_denom: String,
    /// Prefix of bech32 addresses on the sequencer chain
    /// (e.g. "fuelsequencer")
    pub account_prefix: String,
    /// Chain ID of the sequencer chain
    /// (e.g. "fuelsequencer-1")
    pub chain_id: String,
    /// Topic to post blocks to
    pub topic: [u8; 32],
}

impl Config {
    /// Default configuration for locally running shared sequencer node
    pub fn local_node() -> Self {
        Self {
            enabled: false,
            tendermint_api: "http://127.0.0.1:26657".to_owned(),
            blockchain_api: "http://127.0.0.1:1317".to_owned(),
            coin_denom: "utest".to_owned(),
            account_prefix: "fuelsequencer".to_owned(),
            chain_id: "fuelsequencer-1".to_owned(),
            topic: [0u8; 32],
        }
    }

    /// Returns the Cosmos account ID of the sender.
    pub fn sender_account_id<S: Signer>(&self, signer: &S) -> anyhow::Result<AccountId> {
        let sender_public_key = signer.public_key();
        let sender_account_id = sender_public_key
            .account_id(&self.account_prefix)
            .map_err(|err| anyhow!("{err:?}"))?;

        Ok(sender_account_id)
    }
}
