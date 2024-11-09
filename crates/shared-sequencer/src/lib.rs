//! Shared sequencer client

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]

use anyhow::anyhow;
use cosmrs::{
    tx::{
        self,
        Fee,
        MessageExt,
        SignDoc,
        SignerInfo,
    },
    AccountId,
    Coin,
};
use error::{
    CosmosError,
    PostBlobError,
};
use fuel_core_types::blockchain::SealedBlock;
use ports::Signer;
use tendermint_rpc::Client as _;

use http_api::{
    AccountMetadata,
    TopicInfo,
};
use prost::Message;

// Re-exports
pub use prost::bytes::Bytes;

mod config;
mod error;
mod http_api;
pub mod ports;
pub use config::Config;
use fuel_sequencer_proto::proto::fuelsequencer::sequencing::v1::MsgPostBlob;

/// Shared sequencer client
pub struct Client {
    config: config::Config,
}
impl Client {
    /// Create a new shared sequencer client from config.
    pub fn new(config: config::Config) -> Self {
        Self { config }
    }

    fn tendermint(&self) -> anyhow::Result<tendermint_rpc::HttpClient> {
        Ok(tendermint_rpc::HttpClient::new(
            &*self.config.tendermint_api,
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
    pub async fn get_account_meta(
        &self,
        id: AccountId,
    ) -> anyhow::Result<AccountMetadata> {
        http_api::get_account(&self.config.blockchain_api, id).await
    }

    /// Retrieve the topic info by its ID, if it exists
    pub async fn get_topic(&self, id: [u8; 32]) -> anyhow::Result<Option<TopicInfo>> {
        http_api::get_topic(&self.config.blockchain_api, id).await
    }

    /// Post a sealed block to the sequencer chain using some
    /// reasonable defaults and the config.
    /// This is a convenience wrapper for `send_raw`.
    pub async fn send<S: Signer>(
        &self,
        signer: &S,
        block: SealedBlock,
    ) -> anyhow::Result<()> {
        let latest_height = self.latest_block_height().await?;
        let prev_order = self
            .get_topic(self.config.topic)
            .await?
            .map(|f| f.order)
            .unwrap_or_default();

        self.send_raw(
            latest_height.saturating_add(100), // TODO: determine how much this has to be
            100_000,                           // TODO: determine how much this has to be
            signer,
            prev_order.wrapping_add(1),
            self.config.topic,
            Bytes::from(postcard::to_stdvec(&block)?),
        )
        .await
    }

    /// Post a blob of raw data to the sequencer chain
    pub async fn send_raw<S: Signer>(
        &self,
        timeout_height: u32,
        gas_limit: u64,
        signer: &S,
        order: u64,
        topic: [u8; 32],
        data: Bytes,
    ) -> anyhow::Result<()> {
        let amount = Coin {
            amount: gas_limit as u128,
            denom: self.config.coin_denom.parse().map_err(CosmosError)?,
        };

        let fee = Fee::from_amount_and_gas(amount, gas_limit);

        let sender_public_key = signer.public_key();
        let sender_account_id = sender_public_key
            .account_id(&self.config.account_prefix)
            .map_err(CosmosError)?;
        let account = self.get_account_meta(sender_account_id).await?;

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
        let sender_public_key = signer.public_key();
        let sender_account_id = sender_public_key
            .account_id(&self.config.account_prefix)
            .map_err(|err| anyhow!("{err:?}"))?;

        let chain_id = self.config.chain_id.parse()?;

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

        let signer_info =
            SignerInfo::single_direct(Some(sender_public_key), account.sequence);
        let auth_info = signer_info.auth_info(fee);
        let sign_doc =
            SignDoc::new(&tx_body, &auth_info, &chain_id, account.account_number)
                .map_err(|err| anyhow!("{err:?}"))?;

        // let tx_signed = sign_doc.sign(&sender_private_key)?;

        let sign_doc_bytes = sign_doc
            .clone()
            .into_bytes()
            .map_err(|err| anyhow!("{err:?}"))?;
        let signature = &signer.sign(&sign_doc_bytes).await?;

        Ok(cosmos_sdk_proto::cosmos::tx::v1beta1::TxRaw {
            body_bytes: sign_doc.body_bytes,
            auth_info_bytes: sign_doc.auth_info_bytes,
            signatures: vec![signature.to_vec()],
        }
        .to_bytes()?)
    }

    /// Return all blob messages in the given blob
    pub async fn read_block_msgs(&self, height: u32) -> anyhow::Result<Vec<MsgPostBlob>> {
        let mut result = Vec::new();

        let block = self.tendermint()?.block(height).await?;

        for item in block.block.data.iter().skip(1) {
            let tx = cosmrs::Tx::from_bytes(item).map_err(CosmosError)?;
            for msg in tx.body.messages.iter() {
                if msg.type_url == "/fuelsequencer.sequencing.v1.MsgPostBlob" {
                    let msg: MsgPostBlob = prost::Message::decode(msg.value.as_slice())?;
                    result.push(msg);
                }
            }
        }
        Ok(result)
    }
}
