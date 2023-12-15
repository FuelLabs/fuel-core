use std::str::FromStr;

use graph::prelude::{
    lazy_static,
    serde_json::{self, Value},
    web3::{
        api::{Eth, Namespace},
        contract::{tokens::Tokenize, Contract as Web3Contract, Options},
        transports::Http,
        types::{Address, TransactionReceipt},
    },
};
// web3 version 0.18 does not expose this; once the graph crate updates to
// version 0.19, we can use web3::signing::SecretKey from the graph crate
use secp256k1::SecretKey;

use crate::{error, helpers::TestFile, status, CONFIG};

// `FROM` and `FROM_KEY` are the address and private key of the first
// account that anvil prints on startup
lazy_static! {
    static ref FROM: Address =
        Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap();
    static ref FROM_KEY: SecretKey = {
        SecretKey::from_str("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
            .unwrap()
    };
    static ref CONTRACTS: Vec<Contract> = {
        vec![
            Contract {
                name: "SimpleContract".to_string(),
                address: Address::from_str("0x5fbdb2315678afecb367f032d93f642f64180aa3").unwrap(),
            },
            Contract {
                name: "LimitedContract".to_string(),
                address: Address::from_str("0xb7f8bc63bbcad18155201308c8f3540b07f84f5e").unwrap(),
            },
            Contract {
                name: "RevertingContract".to_string(),
                address: Address::from_str("0xa51c1fc2f0d1a1b8494ed1fe312d7c3a78ed91c0").unwrap(),
            },
            Contract {
                name: "OverloadedContract".to_string(),
                address: Address::from_str("0x0dcd1bf9a1b36ce34237eeafef220932846bcd82").unwrap(),
            },
        ]
    };
}

#[derive(Debug, Clone)]
pub struct Contract {
    pub name: String,
    pub address: Address,
}

impl Contract {
    fn eth() -> Eth<Http> {
        let web3 = Http::new(&CONFIG.eth.url()).unwrap();
        Eth::new(web3)
    }

    async fn exists(&self) -> bool {
        let eth = Self::eth();
        let bytes = eth.code(self.address, None).await.unwrap();
        !bytes.0.is_empty()
    }

    fn code_and_abi(name: &str) -> anyhow::Result<(String, Vec<u8>)> {
        let src = TestFile::new(&format!("contracts/src/{}.sol", name));
        let bin = TestFile::new(&format!("contracts/out/{}.sol/{}.json", name, name));
        if src.newer(&bin) {
            println!(
                "The source {} is newer than the compiled contract {}. Please recompile.",
                src, bin
            );
        }

        let json: Value = serde_json::from_reader(bin.reader()?).unwrap();
        let abi = serde_json::to_string(&json["abi"]).unwrap();
        let code = json["bytecode"]["object"].as_str().unwrap();
        Ok((code.to_string(), abi.as_bytes().to_vec()))
    }

    pub async fn deploy(name: &str) -> anyhow::Result<Self> {
        let eth = Self::eth();
        let (code, abi) = Self::code_and_abi(name)?;
        let contract = Web3Contract::deploy(eth, &abi)
            .unwrap()
            .confirmations(1)
            .execute(code, (), FROM.to_owned())
            .await
            .unwrap();

        Ok(Self {
            name: name.to_string(),
            address: contract.address(),
        })
    }

    async fn call(&self, func: &str, params: impl Tokenize) -> anyhow::Result<TransactionReceipt> {
        let eth = Self::eth();
        let (_, abi) = Self::code_and_abi(&self.name)?;
        let contract = Web3Contract::from_json(eth, self.address, &abi)?;
        let options = Options::default();
        let receipt = contract
            .signed_call_with_confirmations(func, params, options, 1, &*FROM_KEY)
            .await
            .unwrap();
        Ok(receipt)
    }

    pub async fn deploy_all() -> anyhow::Result<Vec<Self>> {
        let mut contracts = Vec::new();
        status!("contracts", "Deploying contracts");
        for contract in &*CONTRACTS {
            let mut contract = contract.clone();
            if !contract.exists().await {
                status!(
                    "contracts",
                    "Contract {} does not exist, deploying",
                    contract.name
                );
                let old_address = contract.address;
                contract = Self::deploy(&contract.name).await?;
                if old_address != contract.address {
                    error!(
                        "contracts",
                        "Contract address for {} changed from {:?} to {:?}",
                        contract.name,
                        old_address,
                        contract.address
                    );
                } else {
                    status!(
                        "contracts",
                        "Deployed contract {} at {:?}",
                        contract.name,
                        contract.address
                    );
                }
                // Some tests want 10 calls to `emitTrigger` in place
                if contract.name == "SimpleContract" {
                    status!("contracts", "Calling SimpleContract.emitTrigger 10 times");
                    for i in 1..=10 {
                        contract.call("emitTrigger", (i as u16,)).await.unwrap();
                    }
                }
            } else {
                status!(
                    "contracts",
                    "Contract {} exists at {:?}",
                    contract.name,
                    contract.address
                );
            }
            contracts.push(contract);
        }
        Ok(contracts)
    }
}
