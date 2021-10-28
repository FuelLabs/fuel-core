use fuel_client::client::FuelClient;
use fuel_tx::Transaction;
use serde_json::json;
use structopt::StructOpt;

#[derive(StructOpt)]
enum Command {
    Transaction(TransactionCommands),
}

#[derive(StructOpt)]
enum TransactionCommands {
    Submit { tx: String },
    Get { id: String },
}

#[derive(StructOpt)]
struct CliArgs {
    #[structopt(name = "endpoint", default_value = "127.0.0.1:4000", long = "endpoint")]
    endpoint: String,
    #[structopt(subcommand)]
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
                TransactionCommands::Get { id } => {
                    let tx = client.transaction(id.as_str()).await.unwrap();
                    println!("{:?}", json!(tx).to_string())
                }
            },
        }
    }
}

fn main() {
    futures::executor::block_on(CliArgs::from_args().exec());
}
