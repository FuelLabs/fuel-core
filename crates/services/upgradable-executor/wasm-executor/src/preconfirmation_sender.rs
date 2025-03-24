use fuel_core_executor::ports::PreconfirmationSenderPort;
use fuel_core_types::services::preconfirmation::Preconfirmation;

pub struct PreconfirmationSender;

impl PreconfirmationSenderPort for PreconfirmationSender {
    fn try_send(&self, _: Vec<Preconfirmation>) -> Vec<Preconfirmation> {
        vec![]
    }

    async fn send(&self, _: Vec<Preconfirmation>) {}
}
