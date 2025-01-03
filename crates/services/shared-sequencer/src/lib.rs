//! Shared sequencer client

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]

use anyhow::anyhow;
use cosmrs::{
    tendermint::chain::Id,
    tx::{
        self,
        Fee,
        MessageExt,
        SignDoc,
        SignerInfo,
    },
    AccountId,
    Coin,
    Denom,
};
use error::PostBlobError;
use fuel_sequencer_proto::protos::fuelsequencer::sequencing::v1::MsgPostBlob;
use http_api::{
    AccountMetadata,
    TopicInfo,
};
use ports::Signer;
use prost::Message;
use tendermint_rpc::Client as _;

// Re-exports
pub use config::{
    Config,
    Endpoints,
};
pub use prost::bytes::Bytes;

mod config;
mod error;
mod http_api;
pub mod ports;
pub mod service;

/// Shared sequencer client
pub struct Client {
    endpoints: Endpoints,
    topic: [u8; 32],
    ss_chain_id: Id,
    gas_price: u128,
    coin_denom: Denom,
    account_prefix: String,
}

impl Client {
    /// Create a new shared sequencer client from config.
    pub async fn new(endpoints: Endpoints, topic: [u8; 32]) -> anyhow::Result<Self> {
        let coin_denom = http_api::coin_denom(&endpoints.blockchain_rest_api)
            .await?
            .parse()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let account_prefix =
            http_api::get_account_prefix(&endpoints.blockchain_rest_api).await?;
        let ss_chain_id = http_api::chain_id(&endpoints.blockchain_rest_api)
            .await?
            .parse()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let ss_config = http_api::config(&endpoints.blockchain_rest_api).await?;

        let mut minimum_gas_price = ss_config.minimum_gas_price;

        if let Some(index) = minimum_gas_price.find('.') {
            minimum_gas_price.truncate(index);
        }
        let gas_price: u128 = minimum_gas_price.parse()?;
        // Ceil the gas price to the next integer.
        let gas_price = gas_price.saturating_add(1);

        Ok(Self {
            topic,
            endpoints,
            account_prefix,
            coin_denom,
            ss_chain_id,
            gas_price,
        })
    }

    /// Returns the Cosmos account ID of the sender.
    pub fn sender_account_id<S: Signer>(&self, signer: &S) -> anyhow::Result<AccountId> {
        let sender_public_key = signer.public_key();
        let sender_account_id = sender_public_key
            .account_id(&self.account_prefix)
            .map_err(|err| anyhow!("{err:?}"))?;

        Ok(sender_account_id)
    }

    fn tendermint(&self) -> anyhow::Result<tendermint_rpc::HttpClient> {
        Ok(tendermint_rpc::HttpClient::new(
            &*self.endpoints.tendermint_rpc_api,
        )?)
    }

    /// Retrieve latest block height
    pub async fn latest_block_height(&self) -> anyhow::Result<u32> {
        Ok(self
            .tendermint()?
            .abci_info()
            .await?
            .last_block_height
            .value()
            .try_into()?)
    }

    /// Retrieve account metadata by its ID
    pub async fn get_account_meta<S: Signer>(
        &self,
        signer: &S,
    ) -> anyhow::Result<AccountMetadata> {
        let sender_account_id = self.sender_account_id(signer)?;
        http_api::get_account(&self.endpoints.blockchain_rest_api, sender_account_id)
            .await
    }

    /// Retrieve the topic info, if it exists
    pub async fn get_topic(&self) -> anyhow::Result<Option<TopicInfo>> {
        http_api::get_topic(&self.endpoints.blockchain_rest_api, self.topic).await
    }

    /// Post a sealed block to the sequencer chain using some
    /// reasonable defaults and the config.
    /// This is a convenience wrapper for `send_raw`.
    pub async fn send<S: Signer>(
        &self,
        signer: &S,
        account: AccountMetadata,
        order: u64,
        blob: Vec<u8>,
    ) -> anyhow::Result<()> {
        let latest_height = self.latest_block_height().await?;

        self.send_raw(
            // We don't want our transactions to be stay in the mempool for a long time,
            // so we set the timeout height to be 64 blocks ahead of the latest block height.
            // The 64 is an arbitrary number.
            latest_height.saturating_add(64),
            signer,
            account,
            order,
            self.topic,
            Bytes::from(blob),
        )
        .await
    }

    /// Post a blob of raw data to the sequencer chain
    #[allow(clippy::too_many_arguments)]
    pub async fn send_raw<S: Signer>(
        &self,
        timeout_height: u32,
        signer: &S,
        account: AccountMetadata,
        order: u64,
        topic: [u8; 32],
        data: Bytes,
    ) -> anyhow::Result<()> {
        // We want to estimate the transaction to know what amount and fee to use.
        // We use a dummy amount and fee to estimate the gas, and based on the result
        // we calculate the actual amount and fee to use in real transaction.
        let dummy_amount = Coin {
            amount: 0,
            denom: self.coin_denom.clone(),
        };

        let dummy_fee = Fee::from_amount_and_gas(dummy_amount, 0u64);

        let dummy_payload = self
            .make_payload(
                timeout_height,
                dummy_fee,
                signer,
                account,
                order,
                topic,
                data.clone(),
            )
            .await?;

        let used_gas = http_api::estimate_transaction(
            &self.endpoints.blockchain_rest_api,
            dummy_payload,
        )
        .await?;

        let used_gas = used_gas.saturating_mul(2); // Add some buffer

        let amount = Coin {
            amount: self.gas_price.saturating_mul(used_gas as u128),
            denom: self.coin_denom.clone(),
        };

        let fee = Fee::from_amount_and_gas(amount, used_gas);
        let payload = self
            .make_payload(timeout_height, fee, signer, account, order, topic, data)
            .await?;

        let r = self.tendermint()?.broadcast_tx_sync(payload).await?;
        if r.code.is_err() {
            return Err(PostBlobError { message: r.log }.into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn make_payload<S: Signer>(
        &self,
        timeout_height: u32,
        fee: Fee,
        signer: &S,
        account: AccountMetadata,
        order: u64,
        topic: [u8; 32],
        data: Bytes,
    ) -> anyhow::Result<Vec<u8>> {
        let sender_account_id = self.sender_account_id(signer)?;

        let msg = MsgPostBlob {
            from: sender_account_id.to_string(),
            order: order.to_string(),
            topic: Bytes::from(topic.to_vec()),
            data,
        };
        let any_msg = cosmrs::Any {
            type_url: "/fuelsequencer.sequencing.v1.MsgPostBlob".to_owned(),
            value: msg.encode_to_vec(),
        };
        let tx_body = tx::Body::new(vec![any_msg], "", timeout_height);

        let sender_public_key = signer.public_key();
        let signer_info =
            SignerInfo::single_direct(Some(sender_public_key), account.sequence);
        let auth_info = signer_info.auth_info(fee);
        let sign_doc = SignDoc::new(
            &tx_body,
            &auth_info,
            &self.ss_chain_id,
            account.account_number,
        )
        .map_err(|err| anyhow!("{err:?}"))?;

        let sign_doc_bytes = sign_doc
            .clone()
            .into_bytes()
            .map_err(|err| anyhow!("{err:?}"))?;
        let signature = signer.sign(&sign_doc_bytes).await?;

        // Convert the signature to non-normalized form
        let mut signature_bytes = *signature;
        signature_bytes[32] &= 0x7f;

        Ok(cosmos_sdk_proto::cosmos::tx::v1beta1::TxRaw {
            body_bytes: sign_doc.body_bytes,
            auth_info_bytes: sign_doc.auth_info_bytes,
            signatures: vec![signature_bytes.to_vec()],
        }
        .to_bytes()?)
    }
}
