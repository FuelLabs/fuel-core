use fuel_core_client::client::{
    FuelClient,
    PageDirection,
    PaginationRequest,
};
use fuel_core_types::{
    fuel_crypto::PublicKey,
    fuel_tx::{
        Input,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
    },
    fuel_vm::SecretKey,
};

use crate::config::{
    ClientConfig,
    SuiteConfig,
};

pub struct TestContext {
    pub account_a_client: FuelClient,
    pub account_b_client: FuelClient,
}

impl TestContext {
    pub fn new(config: &SuiteConfig) -> Self {
        let account_a_client =
            Self::new_client(config.endpoint.clone(), &config.wallet_a);
        let account_b_client =
            Self::new_client(config.endpoint.clone(), &config.wallet_b);
        Self {
            account_a_client,
            account_b_client,
        }
    }

    fn new_client(default_endpoint: String, wallet: &ClientConfig) -> FuelClient {
        FuelClient::new(wallet.endpoint.clone().unwrap_or(default_endpoint)).unwrap()
    }
}

pub struct Wallet {
    pub secret: SecretKey,
    pub address: Address,
    pub client: FuelClient,
}

impl Wallet {
    pub fn new(secret: SecretKey, client: FuelClient) -> Self {
        let public_key: PublicKey = (&secret).into();
        let address = Input::owner(&public_key);
        Self {
            secret,
            address,
            client,
        }
    }

    pub async fn balance(&self, asset_id: Option<AssetId>) -> u64 {
        self.client
            .balance(
                &self.address.to_string(),
                Some(asset_id.unwrap_or_default().to_string().as_str()),
            )
            .await
            .unwrap()
    }

    pub async fn owns_coin(&self, utxo_id: UtxoId) -> bool {
        self.client
            .coins(
                &self.address.to_string(),
                None,
                PaginationRequest {
                    cursor: None,
                    results: usize::MAX,
                    direction: PageDirection::Forward,
                },
            )
            .await
            .unwrap()
            .results
            .iter()
            .any(|coin| UtxoId::from(coin.utxo_id.clone()) == utxo_id)
    }
}
