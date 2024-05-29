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

/// Test the `try_subscribe` function with given senders.
///
/// # Arguments
///
/// * `senders` - A HashMap containing a Bytes32 key and a Vec of senders.
fn test_try_subscribe_inner(senders: HashMap<Bytes32, Vec<Sender<(), MockSendStatus>>>) {
    // Set max capacity for number of permits.
    let capacity = 10usize;
    let permits = Arc::new(capacity);
    let model = Arc::new(capacity);

    // Transform the senders HashMap into arcs.
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
                        created: Instant::now(),
                    })
                    .collect(),
            )
        })
        .collect();

    // Calculate the number of closed senders
    let num_closed = senders
        .values()
        .map(|v| v.iter().filter(|v| v.is_closed()).count())
        .sum::<usize>();

    // Clone model Arc pointers to increase the strong count
    // to match the real permits.
    let _model_permits: Vec<_> = (0..Arc::strong_count(&permits)
        // Subtract 1 to account for the Arc pointer in the UpdateSender
        .saturating_sub(1)
        // Subtract the number of closed senders
        .saturating_sub(num_closed))
        .map(|_| Arc::clone(&model))
        .collect();

    // Initialize the UpdateSender.
    let update = UpdateSender {
        senders: Arc::new(Mutex::new(box_senders(senders))),
        permits: Arc::new(permits),
        ttl: Duration::from_secs(100),
    };

    // Test the try_subscribe function on the UpdateSender
    let stream = update.try_subscribe::<MockCreateChannel>(Bytes32::zeroed());

    // Test the try_acquire function on the model Arc
    let model = Arc::new(model);
    let result = model.clone().try_acquire();

    // Assert the equality of model and stream results
    assert_eq!(
        result.is_some(),
        stream.is_some(),
        "model: {} \n{:#?}",
        Arc::strong_count(&Arc::try_unwrap(model).unwrap()),
        update.senders,
    );
}
