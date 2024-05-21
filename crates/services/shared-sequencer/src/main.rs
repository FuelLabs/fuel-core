use fuel_sequencer_client::proto::fuelsequencer::sequencing::v1::msg_client::MsgClient;
use fuel_sequencer_client::proto::fuelsequencer::sequencing::v1::MsgPostBlob;
use fuel_sequencer_client::bytes::Bytes;
use fuel_vm_private::prelude::*;
use tendermint_rpc::{HttpClient, Client};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fuel_core_shared_sequencer_client::xyz().unwrap();

    let client = HttpClient::new("http://127.0.0.1:26657")
        .unwrap();


    let abci_info = client.abci_info()
        .await
        .unwrap();
    println!("Got ABCI info: {:?}", abci_info);

    let reply = client.broadcast_tx_sync(
        bytes
    )
        .await
        .unwrap();
    println!("Submit-reply {reply:?}");

    Ok(())
}
