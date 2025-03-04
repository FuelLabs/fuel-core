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
