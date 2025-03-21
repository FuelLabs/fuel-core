use crate::service::adapters::{
    consensus_module::poa::pre_confirmation_signature::key_generator::Ed25519Key,
    FuelBlockSigner,
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
    signing_key::SigningKey,
};
use fuel_core_types::{
    fuel_tx::Bytes64,
    services::{
        p2p::{
            DelegatePreConfirmationKey,
            PreConfirmationMessage,
            SignedByBlockProducerDelegation,
            SignedPreconfirmationByDelegate,
        },
        preconfirmation::Preconfirmations,
    },
};
use std::sync::Arc;

impl Broadcast for P2PAdapter {
    type ParentKey = FuelBlockSigner;
    type DelegateKey = Ed25519Key;
    type Preconfirmations = Preconfirmations;

    async fn broadcast_preconfirmations(
        &mut self,
        preconfirmations: Self::Preconfirmations,
        signature: <Self::DelegateKey as SigningKey>::Signature,
    ) -> PreConfServiceResult<()> {
        if let Some(p2p) = &self.service {
            let signature_bytes = signature.to_bytes();
            let signature = Bytes64::new(signature_bytes);
            let preconfirmations = Arc::new(PreConfirmationMessage::Preconfirmations(
                SignedPreconfirmationByDelegate {
                    entity: preconfirmations,
                    signature,
                },
            ));
            p2p.broadcast_preconfirmations(preconfirmations)
                .map_err(|e| PreConfServiceError::Broadcast(format!("{e:?}")))?;
        }

        Ok(())
    }

    async fn broadcast_delegate_key(
        &mut self,
        delegate: DelegatePreConfirmationKey<PublicKey<Self>>,
        nonce: u64,
        signature: <Self::ParentKey as ParentSignature>::Signature,
    ) -> PreConfServiceResult<()> {
        if let Some(p2p) = &self.service {
            let seal = SignedByBlockProducerDelegation {
                entity: delegate,
                signature,
            };
            let delegate_key = Arc::new(PreConfirmationMessage::Delegate { seal, nonce });
            p2p.broadcast_preconfirmations(delegate_key)
                .map_err(|e| PreConfServiceError::Broadcast(format!("{e:?}")))?;
        }

        Ok(())
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
    use fuel_core_p2p::{
        ports::P2PPreConfirmationMessage,
        service::{
            build_shared_state,
            TaskRequest,
        },
    };
    use fuel_core_types::{
        ed25519,
        ed25519_dalek::VerifyingKey,
        services::{
            p2p::ProtocolSignature,
            preconfirmation::{
                Preconfirmation,
                PreconfirmationStatus,
            },
        },
        tai64::Tai64,
    };

    #[tokio::test]
    async fn broadcast_preconfirmations__sends_expected_request_over_sender() {
        // given
        let config = fuel_core_p2p::config::Config::default("lolz");
        let (shared_state, mut receiver) = build_shared_state(config);
        let peer_report_config = PeerReportConfig::default();
        let service = Some(shared_state);
        let mut adapter = P2PAdapter::new(service, peer_report_config);
        let preconfirmations = Preconfirmations {
            preconfirmations: vec![Preconfirmation {
                tx_id: Default::default(),
                status: PreconfirmationStatus::Failure {
                    tx_pointer: Default::default(),
                    total_gas: 0,
                    total_fee: 0,
                    receipts: vec![],
                    outputs: vec![],
                },
            }],
            expiration: Tai64::UNIX_EPOCH,
        };
        let signature = ed25519::Signature::from_bytes(&[5u8; 64]);

        // when
        adapter
            .broadcast_preconfirmations(preconfirmations.clone(), signature)
            .await
            .unwrap();

        // then
        let actual = receiver.recv().await.unwrap();
        assert!(matches!(
            actual,
            TaskRequest::BroadcastPreConfirmations(inner)
            if pre_conf_matches_expected_values(
                &inner,
                &preconfirmations.preconfirmations,
                &Bytes64::new(signature.to_bytes()),
                preconfirmations.expiration,
            )
        ));
    }

    fn pre_conf_matches_expected_values(
        inner: &Arc<P2PPreConfirmationMessage>,
        preconfirmations: &[Preconfirmation],
        signature: &Bytes64,
        expiration: Tai64,
    ) -> bool {
        match &**inner {
            PreConfirmationMessage::Preconfirmations(signed_preconfirmation) => {
                signed_preconfirmation.entity.preconfirmations == preconfirmations
                    && signed_preconfirmation.entity.expiration == expiration
                    && signed_preconfirmation.signature == *signature
            }
            _ => false,
        }
    }

    #[tokio::test]
    async fn broadcast_delegate_key__sends_expected_request_over_sender() {
        // given
        let config = fuel_core_p2p::config::Config::default("lolz");
        let (shared_state, mut receiver) = build_shared_state(config);
        let peer_report_config = PeerReportConfig::default();
        let service = Some(shared_state);
        let mut adapter = P2PAdapter::new(service, peer_report_config);
        let expiration = Tai64::UNIX_EPOCH;
        let delegate = DelegatePreConfirmationKey {
            public_key: Default::default(),
            expiration,
        };
        let signature = ProtocolSignature::from_bytes([5u8; 64]);

        // when
        adapter
            .broadcast_delegate_key(delegate.clone(), 0, signature)
            .await
            .unwrap();

        // then
        let actual = receiver.recv().await.unwrap();
        assert!(matches!(
            actual,
            TaskRequest::BroadcastPreConfirmations(inner)
            if delegate_keys_matches_expected_values(
                &inner,
                delegate.public_key,
                expiration,
                &signature,
            )
        ));
    }

    fn delegate_keys_matches_expected_values(
        inner: &Arc<P2PPreConfirmationMessage>,
        delegate_key: VerifyingKey,
        expiration: Tai64,
        signature: &ProtocolSignature,
    ) -> bool {
        let entity = DelegatePreConfirmationKey {
            public_key: delegate_key,
            expiration,
        };
        match &**inner {
            PreConfirmationMessage::Delegate {
                seal: signed_by_block_producer_delegation,
                ..
            } => {
                signed_by_block_producer_delegation.entity == entity
                    && signed_by_block_producer_delegation.signature == *signature
            }
            _ => false,
        }
    }
}
