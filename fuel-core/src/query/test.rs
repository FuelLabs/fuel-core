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

#[tokio::test]
async fn can_build_output_proof() {
    use mockall::predicate::*;
    static RECEIPTS: [Receipt; 3] = [receipt(Some(10)), receipt(None), receipt(Some(3))];
    static TXNS: [Bytes32; 4] = [txn_id(20), txn_id(24), txn_id(1), txn_id(33)];
    let mut data = MockDataSource::new();
    let transaction_id = Default::default();
    data.expect_receipts()
        .once()
        .with(eq(transaction_id))
        .return_const(RECEIPTS.iter());
    data.expect_transaction_status()
        .with(eq(transaction_id))
        .returning(|_| {
            Some(TransactionStatus::Success {
                block_id: Default::default(),
                time: Default::default(),
                result: ProgramState::Return(Default::default()),
            })
        });
    data.expect_transactions_on_block()
        .once()
        .with(eq(Bytes32::default()))
        .return_const(TXNS.iter());
    data.expect_transaction().returning(|txn_id| {
        TXNS.iter().find(|t| *t == txn_id).map(|id| {
            let mut txn = Transaction::default();
            match &mut txn {
                Transaction::Script { outputs, .. }
                | Transaction::Create { outputs, .. } => todo!(),
                _ => todo!(),
            }
            txn
        })
    });

    let message_id = message_id(3);
    let p = output_proof(&data, transaction_id, message_id)
        .await
        .unwrap();
}
