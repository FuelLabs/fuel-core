use super::*;

const fn message_id(i: u8) -> MessageId {
    MessageId::new([i; 32])
}

const fn txn_id(i: u8) -> Bytes32 {
    Bytes32::new([i; 32])
}

const fn receipt(i: Option<u8>) -> Receipt {
    match i {
        Some(i) => Receipt::MessageOut {
            message_id: message_id(i),
            sender: Address::new([0; 32]),
            recipient: Address::new([0; 32]),
            amount: 0,
            nonce: Bytes32::new([0; 32]),
            len: 0,
            digest: Bytes32::new([0; 32]),
            data: Vec::new(),
        },
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

const fn receipt_from_id(message_id: MessageId) -> Receipt {
    Receipt::MessageOut {
        message_id,
        sender: Address::new([0; 32]),
        recipient: Address::new([0; 32]),
        amount: 0,
        nonce: Bytes32::new([0; 32]),
        len: 0,
        digest: Bytes32::new([0; 32]),
        data: Vec::new(),
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
    static MESSAGE_ID: MessageId = MessageId::new([
        91, 111, 181, 142, 97, 250, 71, 89, 57, 118, 125, 104, 164, 70, 249, 127, 27,
        255, 2, 192, 229, 147, 90, 62, 168, 187, 81, 230, 81, 87, 131, 216,
    ]);
    static RECEIPTS: [Receipt; 4] = [
        receipt(Some(10)),
        receipt(None),
        receipt(Some(3)),
        receipt_from_id(MESSAGE_ID),
    ];
    static TXNS: [Bytes32; 4] = [txn_id(20), txn_id(24), txn_id(1), txn_id(33)];
    static OTHER_RECEIPTS: [Receipt; 4] = [
        receipt(Some(4)),
        receipt(Some(5)),
        receipt(Some(6)),
        receipt(Some(7)),
    ];

    let mut out = (0..1)
        .flat_map(|_| vec![message_out(), other_out()])
        .cycle();
    let mut data = MockMessageProofData::new();
    let transaction_id = txn_id(33);
    let mut count = 0;
    data.expect_receipts().returning(move |txn_id| {
        if *txn_id == transaction_id {
            Ok(RECEIPTS.to_vec())
        } else {
            let r = OTHER_RECEIPTS[count..=count].to_vec();
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
            let mut txn = Transaction::default();
            match &mut txn {
                Transaction::Script { outputs, .. }
                | Transaction::Create { outputs, .. } => {
                    outputs.push(out.next().unwrap());
                    outputs.push(out.next().unwrap());
                }
            }
            txn
        }))
    });

    data.expect_message()
        .once()
        .with(eq(MESSAGE_ID))
        .returning(|_| Ok(Some(fuel_core_interfaces::model::Message::default())));

    data.expect_signature()
        .once()
        .with(eq(Bytes32::default()))
        .returning(|_| Ok(Some(fuel_crypto::Signature::default())));

    data.expect_block()
        .once()
        .with(eq(Bytes32::default()))
        .returning(|_| Ok(Some(FuelBlockDb::default())));

    let p = message_proof(&data, transaction_id, MESSAGE_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.message.id(), MESSAGE_ID);
    assert_eq!(p.signature, fuel_crypto::Signature::default());
    assert_eq!(p.block.id(), FuelBlockDb::default().id());
}
