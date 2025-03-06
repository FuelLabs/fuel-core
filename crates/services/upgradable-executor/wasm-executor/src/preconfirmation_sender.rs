use fuel_core_executor::ports::PreconfirmationSenderPort;
use fuel_core_types::services::preconfirmation::PreconfirmationStatus;

pub struct PreconfirmationSender;

impl PreconfirmationSenderPort for PreconfirmationSender {
    fn try_send(&self, _: Vec<PreconfirmationStatus>) -> Vec<PreconfirmationStatus> {
        vec![]
    }

    async fn send(&self, _: Vec<PreconfirmationStatus>) {}
}
