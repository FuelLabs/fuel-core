use std::collections::HashMap;

use fuel_core_interfaces::{
    common::fuel_tx::Script,
    model::{
        FuelApplicationHeader,
        FuelConsensusHeader,
        PartialFuelBlockHeader,
    },
};

use super::*;

const fn txn_id(i: u8) -> Bytes32 {
    Bytes32::new([i; 32])
}

fn receipt(i: Option<u8>) -> Receipt {
    match i {
        Some(i) => {
            let sender = Address::new([i; 32]);
            let recipient = Address::new([i; 32]);
            let amount = 0;
            let nonce = Bytes32::new([i; 32]);
            let data = Vec::new();
            let message_id =
                Output::message_id(&sender, &recipient, &nonce, amount, &data);
            Receipt::MessageOut {
                message_id,
                len: 0,
                digest: Bytes32::new([0; 32]),
                sender,
                recipient,
                amount,
                nonce,
                data,
            }
        }
        None => Receipt::Call {
            id: ContractId::new([0; 32]),
            to: ContractId::new([0; 32]),
            amount: 0,
            asset_id: AssetId::new([0; 32]),
            gas: 0,
            param1: 0,
            param2: 0,
            pc: 0,
            is: 0,
        },
    }
}

fn message_out() -> Output {
    Output::Message {
        recipient: Default::default(),
        amount: Default::default(),
    }
}

fn other_out() -> Output {
    Output::Coin {
        to: Default::default(),
        amount: Default::default(),
        asset_id: Default::default(),
    }
}

#[tokio::test]
async fn can_build_message_proof() {
    use mockall::predicate::*;
    let expected_receipt = receipt(Some(11));
    let message_id = *expected_receipt.message_id().unwrap();
    let receipts: [Receipt; 4] = [
        receipt(Some(10)),
        receipt(None),
        receipt(Some(3)),
        expected_receipt,
    ];
    static TXNS: [Bytes32; 4] = [txn_id(20), txn_id(24), txn_id(1), txn_id(33)];
    let transaction_id = TXNS[3];
    let other_receipts: [Receipt; 3] =
        [receipt(Some(4)), receipt(Some(5)), receipt(Some(6))];

    let message_ids: Vec<MessageId> = other_receipts
        .iter()
        .chain(receipts.iter())
        .filter_map(|r| Some(*r.message_id()?))
        .collect();

    let mut out = HashMap::new();
    out.insert(
        transaction_id,
        vec![message_out(), other_out(), message_out(), message_out()],
    );
    out.insert(TXNS[0], vec![message_out(), other_out()]);
    out.insert(TXNS[1], vec![message_out(), other_out()]);
    out.insert(TXNS[2], vec![message_out(), other_out()]);

    let mut data = MockMessageProofData::new();
    let mut count = 0;

    data.expect_receipts().returning(move |txn_id| {
        if *txn_id == transaction_id {
            Ok(receipts.to_vec())
        } else {
            let r = other_receipts[count..=count].to_vec();
            count += 1;
            Ok(r)
        }
    });
    data.expect_transaction_status()
        .with(eq(transaction_id))
        .returning(|_| {
            Ok(Some(TransactionStatus::Success {
                block_id: Default::default(),
                time: Default::default(),
                result: ProgramState::Return(Default::default()),
            }))
        });
    data.expect_transactions_on_block()
        .once()
        .with(eq(Bytes32::default()))
        .returning(|_| Ok(TXNS.to_vec()));

    data.expect_transaction().returning(move |txn_id| {
        Ok(TXNS.iter().find(|t| *t == txn_id).map(|_| {
            let mut txn = Script::default();
            txn.outputs_mut().extend(out.get(txn_id).unwrap());
            txn.into()
        }))
    });

    data.expect_signature()
        .once()
        .with(eq(Bytes32::default()))
        .returning(|_| Ok(Some(fuel_crypto::Signature::default())));

    let header = PartialFuelBlockHeader {
        application: FuelApplicationHeader {
            da_height: 0u64.into(),
            generated: Default::default(),
        },
        consensus: FuelConsensusHeader {
            prev_root: Bytes32::zeroed(),
            height: 1u64.into(),
            time: Default::default(),
            generated: Default::default(),
        },
        metadata: None,
    };
    data.expect_block()
        .once()
        .with(eq(Bytes32::default()))
        .returning({
            let header = header.clone();
            let message_ids = message_ids.clone();
            move |_| {
                let header = header.clone().generate(&[vec![]], &message_ids);
                let transactions = TXNS.to_vec();
                Ok(Some(FuelBlockDb {
                    header,
                    transactions,
                }))
            }
        });

    let p = message_proof(&data, transaction_id, message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.message_id(), message_id);
    assert_eq!(p.signature, fuel_crypto::Signature::default());
    let header = header.generate(&[vec![]], &message_ids);
    assert_eq!(p.header.output_messages_root, header.output_messages_root);
}
