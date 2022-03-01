use clap::Parser;
use fuel_gql_client::client::FuelClient;
use fuel_tx::Transaction;
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
    /// Submit a JSON encoded transaction for a dry-run execution
    DryRun { tx: String },
    /// Get the transactions associated with a particular transaction id
    Get { id: String },
    /// Get the receipts for a particular transaction id
    Receipts { id: String },
}

#[derive(Parser)]
struct CliArgs {
    #[clap(name = "endpoint", default_value = "127.0.0.1:4000", long = "endpoint")]
    endpoint: String,
    #[clap(subcommand)]
    command: Command,
}

impl CliArgs {
    async fn exec(&self) {
        let client = FuelClient::new(self.endpoint.as_str()).expect("expected valid endpoint");

        match &self.command {
            Command::Transaction(sub_cmd) => match sub_cmd {
                TransactionCommands::Submit { tx } => {
                    let tx: Transaction =
                        serde_json::from_str(tx).expect("invalid transaction json");

                    let result = client.submit(&tx).await;
                    println!("{}", result.unwrap().0);
                }
                TransactionCommands::DryRun { tx } => {
                    let tx: Transaction =
                        serde_json::from_str(tx).expect("invalid transaction json");

                    let result = client.dry_run(&tx).await;
                    println!("{:?}", result.unwrap());
                }
                TransactionCommands::Get { id } => {
                    let tx = client.transaction(id.as_str()).await.unwrap();
                    println!("{:?}", json!(tx).to_string())
                }
                TransactionCommands::Receipts { id } => {
                    let receipts = client.receipts(id.as_str()).await.unwrap();
                    println!("{:?}", json!(receipts).to_string())
                }
            },
        }
    }
}

fn main() {
    futures::executor::block_on(CliArgs::parse().exec());
}
