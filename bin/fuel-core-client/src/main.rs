#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use clap::Parser;
use fuel_core_client::client::FuelClient;
use fuel_core_types::fuel_tx::{Transaction, TxId};
use serde_json::json;

#[derive(Parser)]
enum Command {
    #[clap(subcommand)]
    Transaction(TransactionCommands),
}

#[derive(Parser)]
enum TransactionCommands {
    /// Submit a JSON encoded transaction for inclusion in a block
    Submit { tx: String },
    /// Submit a JSON encoded transaction for predicate estimation.
    EstimatePredicates { tx: String },
    /// Submit a JSON encoded transaction for a dry-run execution
    DryRun { txs: Vec<String> },
    /// Get the transactions associated with a particular transaction id
    Get { id: String },
    /// Get the receipts for a particular transaction id
    Receipts { id: String },
}

#[derive(Parser)]
#[clap(name = "fuel-gql-cli", about = "Fuel GraphQL Endpoint CLI", version)]
struct CliArgs {
    #[clap(name = "endpoint", default_value = "127.0.0.1:4000", long = "endpoint")]
    endpoint: String,
    #[clap(subcommand)]
    command: Command,
}

impl CliArgs {
    async fn exec(&self) {
        let client =
            FuelClient::new(self.endpoint.as_str()).expect("expected valid endpoint");

        match &self.command {
            Command::Transaction(sub_cmd) => match sub_cmd {
                TransactionCommands::Submit { tx } => {
                    let tx: Transaction =
                        serde_json::from_str(tx).expect("invalid transaction json");

                    let result = client.submit(&tx).await;
                    println!("{}", result.unwrap());
                }
                TransactionCommands::EstimatePredicates { tx } => {
                    let mut tx: Transaction =
                        serde_json::from_str(tx).expect("invalid transaction json");

                    client
                        .estimate_predicates(&mut tx)
                        .await
                        .expect("Should be able to estimate predicates");
                    println!("{:?}", tx);
                }
                TransactionCommands::DryRun { txs } => {
                    let txs: Vec<Transaction> = txs
                        .iter()
                        .map(|tx| {
                            serde_json::from_str(tx).expect("invalid transaction json")
                        })
                        .collect();

                    let result = client.dry_run(&txs).await;
                    println!("{:?}", result.unwrap());
                }
                TransactionCommands::Get { id } => {
                    let tx_id = id.parse::<TxId>().expect("invalid transaction id");
                    let tx = client.transaction(&tx_id).await.unwrap();
                    println!("{:?}", json!(tx).to_string())
                }
                TransactionCommands::Receipts { id } => {
                    let tx_id = id.parse::<TxId>().expect("invalid transaction id");
                    let receipts = client.receipts(&tx_id).await.unwrap().unwrap();
                    println!("{:?}", json!(receipts).to_string())
                }
            },
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    CliArgs::parse().exec().await;
}
