use fuel_core_executor::ports::PreconfirmationSenderPort;
use fuel_core_types::services::preconfirmation::PreConfirmationStatus;

pub struct PreconfirmationSender;

impl PreconfirmationSenderPort for PreconfirmationSender {
    fn try_send(&self, _: Vec<PreConfirmationStatus>) -> Vec<PreConfirmationStatus> {
        vec![]
    }

    async fn send(&self, _: Vec<PreConfirmationStatus>) {}
}
