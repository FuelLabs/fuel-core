#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
// #![deny(unused_crate_dependencies)]
// #![deny(missing_docs)]

use bip32::{Language, Mnemonic};
use cosmrs::{
    bank::MsgSend,
    crypto::secp256k1,
    tx::{self, Fee, Msg, SignDoc, SignerInfo, Tx},
    AccountId, Coin, ErrorReport,
};
use fuel_sequencer_client::bytes::Bytes;
use fuel_sequencer_client::proto::fuelsequencer::sequencing::v1::MsgPostBlob;
use fuel_vm_private::prelude::*;
use prost::Message;

pub mod config;

pub fn xyz() -> Result<Vec<u8>, ErrorReport> {
    let sk = fuel_vm_private::fuel_crypto::SecretKey::new_from_mnemonic_phrase_with_path(
        "bar describe panda mosquito quiz room daring round nurse disagree swallow frown hat repeat recall flight skin sketch volume dutch range grunt assist nerve",
        "m/44'/118'/0'/0/0",
    ).unwrap();

    let sender_private_key = secp256k1::SigningKey::from_slice(&*sk).unwrap();
    let sender_public_key = sender_private_key.public_key();
    let sender_account_id = sender_public_key.account_id("fuelsequencer")?;

    dbg!(&sender_account_id);

    let amount = Coin {
        amount: 1_000_000u128,
        denom: "utest".parse()?,
    };

    let chain_id = "fuelsequencer-1".parse()?;
    let account_number = 2;
    let sequence_number = 10;
    let gas = 100_000u64;

    let msg = MsgPostBlob {
        from: sender_account_id.to_string(),
        order: 0.to_string(), // TODO: this is an argument
        topic: "data".into(), // TODO: figure out what goes here
        data: Bytes::from(b"Lorem ipsum dolor sit amet".to_vec()),
    };
    let any_msg = cosmrs::Any {
        type_url: "/fuelsequencer.sequencing.v1.MsgPostBlob".to_string(),
        value: msg.encode_to_vec(),
    };
    let tx_body = tx::Body::new(vec![any_msg], "example memo", 1200u32);

    let signer_info = SignerInfo::single_direct(Some(sender_public_key), sequence_number);
    let auth_info = signer_info.auth_info(Fee::from_amount_and_gas(amount, gas));
    let sign_doc = SignDoc::new(&tx_body, &auth_info, &chain_id, account_number)?;
    let tx_signed = sign_doc.sign(&sender_private_key)?;
    tx_signed.to_bytes()
}
