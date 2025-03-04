use crate::service::adapters::{
    consensus_module::poa::pre_confirmation_signature::{
        key_generator::Ed25519Key,
        parent_signature::FuelParentSigner,
    },
    P2PAdapter,
};
use fuel_core_poa::pre_confirmation_signature_service::{
    broadcast::{
        Broadcast,
        PublicKey,
    },
    error::{
        Error as PreConfServiceError,
        Result as PreConfServiceResult,
    },
    parent_signature::ParentSignature,
};
use fuel_core_types::{
    services::p2p::{
        DelegatePreConfirmationKey,
        PreConfirmationMessage,
        Preconfirmation,
        Preconfirmations,
        SignedPreconfirmationByDelegate,
    },
    tai64::Tai64,
};

use fuel_core_poa::pre_confirmation_signature_service::signing_key::SigningKey;
use fuel_core_types::fuel_crypto::Signature;
use std::sync::Arc;

impl Broadcast for P2PAdapter {
    type ParentKey = FuelParentSigner;
    type DelegateKey = Ed25519Key;
    type Preconfirmations = Vec<Preconfirmation>;

    async fn broadcast_preconfirmations(
        &mut self,
        preconfirmations: Self::Preconfirmations,
        signature: <Self::DelegateKey as SigningKey>::Signature,
        expiration: Tai64,
    ) -> PreConfServiceResult<()> {
        if let Some(p2p) = &self.service {
            let entity = Preconfirmations {
                expiration,
                preconfirmations,
            };
            let signature_bytes = signature.to_bytes();
            let signature = Signature::from_bytes(signature_bytes);
            let preconfirmations = Arc::new(PreConfirmationMessage::Preconfirmations(
                SignedPreconfirmationByDelegate { entity, signature },
            ));
            p2p.broadcast_preconfirmations(preconfirmations)
                .map_err(|e| PreConfServiceError::Broadcast(format!("{e:?}")))?;
            Ok(())
        } else {
            Err(PreConfServiceError::Broadcast(
                "P2P service not available".to_string(),
            ))
        }
    }

    async fn broadcast_delegate_key(
        &mut self,
        _: DelegatePreConfirmationKey<PublicKey<Self>>,
        _: <Self::ParentKey as ParentSignature>::Signature,
    ) -> PreConfServiceResult<()> {
        todo!()
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;

    use crate::service::adapters::{
        P2PAdapter,
        PeerReportConfig,
    };
    use fuel_core_p2p::service::{
        build_shared_state,
        TaskRequest,
    };
    use fuel_core_types::services::p2p::PreconfirmationStatus;

    #[tokio::test]
    async fn broadcast_preconfirmations__sends_expected_request_over_sender() {
        // given
        let config = fuel_core_p2p::config::Config::default("lolz");
        let (shared_state, mut receiver) = build_shared_state(config);
        let peer_report_config = PeerReportConfig::default();
        let service = Some(shared_state);
        let mut adapter = P2PAdapter::new(service, peer_report_config);
        let preconfirmations = vec![Preconfirmation {
            tx_id: Default::default(),
            status: PreconfirmationStatus::FailureByBlockProducer {
                block_height: Default::default(),
            },
        }];
        let signature = ed25519::Signature::from_bytes(&[5u8; 64]);
        let expiration = Tai64::UNIX_EPOCH;

        // when
        adapter
            .broadcast_preconfirmations(preconfirmations.clone(), signature, expiration)
            .await
            .unwrap();

        // then
        let actual = receiver.recv().await.unwrap();
        assert!(matches!(
            actual,
            TaskRequest::BroadcastPreConfirmations(inner)
            if inner_matches_expected_values(
                &inner,
                &preconfirmations,
                &Signature::from_bytes(signature.to_bytes()),
                &expiration,
            )
        ));
    }

    fn inner_matches_expected_values(
        inner: &Arc<PreConfirmationMessage>,
        preconfirmations: &[Preconfirmation],
        signature: &Signature,
        expiration: &Tai64,
    ) -> bool {
        let entity = Preconfirmations {
            expiration: *expiration,
            preconfirmations: preconfirmations.to_vec(),
        };
        match &**inner {
            PreConfirmationMessage::Preconfirmations(signed_preconfirmation) => {
                signed_preconfirmation.entity == entity
                    && signed_preconfirmation.signature == *signature
            }
            _ => false,
        }
    }
}
