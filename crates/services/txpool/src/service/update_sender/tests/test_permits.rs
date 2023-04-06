use crate::service::update_sender::tests::utils::{
    box_senders,
    MockCreateChannel,
};

use super::{
    utils::{
        construct_senders,
        senders_strategy_all_ok,
        SenderData,
    },
    *,
};

impl PermitTrait for Arc<usize> {}

impl Permits for Arc<usize> {
    fn try_acquire(self: Arc<Self>) -> Option<Permit> {
        (std::sync::Arc::<usize>::strong_count(&self) < **self).then(|| {
            let p: Permit =
                Box::new(std::convert::AsRef::<Arc<usize>>::as_ref(&self).clone());
            p
        })
    }
    fn acquire(self: Arc<Self>) -> Pin<Box<dyn Future<Output = Permit> + Send + Sync>> {
        unimplemented!()
    }
}

#[proptest]
fn test_try_subscribe(
    #[strategy(senders_strategy_all_ok())] senders: HashMap<
        Bytes32,
        Vec<Sender<(), MockSendStatus>>,
    >,
) {
    test_try_subscribe_inner(senders);
}

#[test]
fn test_try_subscribe_reg() {
    test_try_subscribe_inner(construct_senders(&[
        (4, &[SenderData::empty_ok()]),
        (1, &[SenderData::empty_ok()]),
        (3, &[SenderData::empty_ok()]),
    ]));
}

fn test_try_subscribe_inner(senders: HashMap<Bytes32, Vec<Sender<(), MockSendStatus>>>) {
    let capacity = 10usize;
    let permits = Arc::new(capacity);
    let model = Arc::new(capacity);
    let senders: HashMap<_, Vec<_>> = senders
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                v.into_iter()
                    .map(|v| Sender::<Arc<usize>, _> {
                        _permit: Arc::clone(&permits),
                        stream: v.stream,
                        tx: v.tx,
                    })
                    .collect(),
            )
        })
        .collect();
    let num_closed = senders
        .values()
        .map(|v| v.iter().filter(|v| v.is_closed()).count())
        .sum::<usize>();
    let _model_permits: Vec<_> = (0..Arc::strong_count(&permits)
        .saturating_sub(1)
        .saturating_sub(num_closed))
        .map(|_| Arc::clone(&model))
        .collect();
    let update = UpdateSender {
        senders: Arc::new(Mutex::new(box_senders(senders))),
        permits: Arc::new(permits),
    };

    let stream = update.try_subscribe::<MockCreateChannel>(Bytes32::zeroed());
    let model = Arc::new(model);
    let result = model.clone().try_acquire();
    assert_eq!(
        result.is_some(),
        stream.is_some(),
        "model: {} \n{:#?}",
        Arc::strong_count(&Arc::try_unwrap(model).unwrap()),
        update.senders,
    );
}
