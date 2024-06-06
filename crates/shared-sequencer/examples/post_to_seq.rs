// use std::time::SystemTime;

// use cosmrs::crypto::secp256k1;
// use fuel_core_shared_sequencer_client::{
//     proto::fuelsequencer::sequencing::v1::{
//         msg_client::MsgClient,
//         query_client::QueryClient,
//         MsgPostBlob,
//         QueryGetTopicRequest,
//     },
//     Bytes,
//     Client,
//     Config,
// };
// use fuel_vm_private::prelude::*;

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // let sk = fuel_vm_private::fuel_crypto::SecretKey::new_from_mnemonic_phrase_with_path(
//     //     "bar describe panda mosquito quiz room daring round nurse disagree swallow frown hat repeat recall flight skin sketch volume dutch range grunt assist nerve",
//     //     "m/44'/118'/0'/0/0",
//     // ).unwrap();

//     let sk = fuel_vm_private::fuel_crypto::SecretKey::new_from_mnemonic_phrase_with_path(
//         "dinner crash nurse casino baby fold race cheese elite column sausage sleep close royal rain over mechanic minimum outdoor conduct cash wagon frog evidence",
//         "m/44'/118'/0'/0/0",
//     ).unwrap();

//     let sender_private_key = secp256k1::SigningKey::from_slice(&*sk).unwrap();
//     let sender_public_key = sender_private_key.public_key();
//     let sender_account_id = sender_public_key.account_id("fuelsequencer")?;

//     let client = Client::new(Config {
//         // tendermint_api: "https://rpc-seq.simplystaking.xyz".to_owned(),
//         // blockchain_api: "https://rest-seq.simplystaking.xyz/".to_owned(),
//         // chain_id: "seq-devnet-3".to_owned(),
//         ..Config::local_node()
//     });

//     let latest_block_height = client.latest_block_height().await?;
//     dbg!(latest_block_height);
//     dbg!(client.get_account_meta(sender_account_id).await?);

//     let topic = [1; 32];

//     let topic_order = client
//         .get_topic(topic)
//         .await?
//         .map(|t| t.order)
//         .unwrap_or_default()
//         + 1;

//     let time = SystemTime::now()
//         .duration_since(SystemTime::UNIX_EPOCH)
//         .unwrap()
//         .as_secs();
//     let msg = format!("example,timestamp={time:?}");
//     client
//         .send_raw(
//             latest_block_height as u32 + 100,
//             100_000,
//             sender_private_key,
//             topic_order,
//             topic,
//             Bytes::from(msg.as_str().to_owned()),
//         )
//         .await?;

//     tokio::time::sleep(std::time::Duration::from_secs(1)).await;

//     'outer: for r in latest_block_height.. {
//         loop {
//             if let Ok(items) = client.read_block_msgs(r as u32).await {
//                 dbg!(&items);
//                 if items.len() != 0 {
//                     break 'outer;
//                 }
//                 break;
//             } else {
//                 println!("Error reading block messages, retrying in 3 seconds...");
//                 tokio::time::sleep(std::time::Duration::from_secs(3)).await;
//             }
//         }
//     }

//     Ok(())
// }

fn main() {}
