use fuel_core_tx_status_manager::service::SignatureVerification;
use fuel_core_types::{
    ed25519::Signature,
    ed25519_dalek::Verifier,
    fuel_crypto::{
        Message,
        PublicKey,
    },
    fuel_tx::Bytes64,
    services::{
        p2p::{
            DelegatePreConfirmationKey,
            DelegatePublicKey,
            ProtocolSignature,
            Sealed,
        },
        preconfirmation::Preconfirmations,
    },
    tai64::Tai64,
};
use std::collections::HashMap;

pub struct PreconfirmationSignatureVerification {
    protocol_pubkey: PublicKey,
    delegate_keys: HashMap<Tai64, DelegatePublicKey>,
}

impl PreconfirmationSignatureVerification {
    pub fn new(protocol_pubkey: PublicKey) -> Self {
        Self {
            protocol_pubkey,
            delegate_keys: HashMap::new(),
        }
    }

    fn verify_preconfirmation(
        delegate_key: &DelegatePublicKey,
        sealed: &Sealed<Preconfirmations, Bytes64>,
    ) -> bool {
        let bytes = match postcard::to_allocvec(&sealed.entity) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!("Failed to serialize preconfirmation: {e:?}");
                return false;
            }
        };

        let signature = Signature::from_bytes(&sealed.signature);
        match delegate_key.verify(&bytes, &signature) {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("Failed to verify preconfirmation signature: {e:?}");
                false
            }
        }
    }

    fn remove_expired_delegates(&mut self) {
        let now = Tai64::now();
        self.delegate_keys.retain(|exp, _| exp > &now);
    }
}

impl SignatureVerification for PreconfirmationSignatureVerification {
    async fn add_new_delegate(
        &mut self,
        sealed: &Sealed<DelegatePreConfirmationKey<DelegatePublicKey>, ProtocolSignature>,
    ) -> bool {
        let Sealed { entity, signature } = sealed;
        let bytes = postcard::to_allocvec(&entity).unwrap();
        let message = Message::new(&bytes);
        let verified = signature.verify(&self.protocol_pubkey, &message);
        self.remove_expired_delegates();
        match verified {
            Ok(_) => {
                self.delegate_keys
                    .insert(entity.expiration, entity.public_key);
                true
            }
            Err(_) => false,
        }
    }

    async fn check_preconfirmation_signature(
        &mut self,
        sealed: &Sealed<Preconfirmations, Bytes64>,
    ) -> bool {
        let expiration = sealed.entity.expiration;
        let now = Tai64::now();
        if now > expiration {
            tracing::warn!("Preconfirmation signature expired: {now:?} > {expiration:?}");
            return false;
        }
        self.delegate_keys
            .get(&expiration)
            .map(|delegate_key| Self::verify_preconfirmation(delegate_key, sealed))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use super::*;
    use fuel_core_types::{
        ed25519_dalek::{
            Signer,
            SigningKey as DalekSigningKey,
            VerifyingKey as DalekVerifyingKey,
        },
        fuel_crypto::{
            Message,
            SecretKey,
            Signature,
        },
        tai64::Tai64,
    };

    fn valid_sealed_delegate_signature(
        protocol_secret_key: SecretKey,
        delegate_public_key: DelegatePublicKey,
        expiration: Tai64,
    ) -> Sealed<DelegatePreConfirmationKey<DelegatePublicKey>, ProtocolSignature> {
        let entity = DelegatePreConfirmationKey {
            public_key: delegate_public_key,
            expiration,
        };
        let bytes = postcard::to_allocvec(&entity).unwrap();
        let message = Message::new(&bytes);
        let signature = Signature::sign(&protocol_secret_key, &message);
        Sealed { entity, signature }
    }

    fn arb_valid_sealed_delegate_signature(
        protocol_secret_key: SecretKey,
    ) -> Sealed<DelegatePreConfirmationKey<DelegatePublicKey>, ProtocolSignature> {
        let delegate_public_key = DelegatePublicKey::default();
        let expiration = Tai64(u64::MAX);
        valid_sealed_delegate_signature(
            protocol_secret_key,
            delegate_public_key,
            expiration,
        )
    }

    fn valid_pre_confirmation_signature(
        delegate_private_key: DalekSigningKey,
        expiration: Tai64,
    ) -> Sealed<Preconfirmations, Bytes64> {
        let entity = Preconfirmations {
            expiration,
            preconfirmations: vec![],
        };
        let bytes = postcard::to_allocvec(&entity).unwrap();
        let typed_signature = delegate_private_key.sign(&bytes);
        let signature = Bytes64::new(typed_signature.to_bytes());
        Sealed { entity, signature }
    }

    fn bad_pre_confirmation_signature(
        delegate_private_key: DalekSigningKey,
        expiration: Tai64,
    ) -> Sealed<Preconfirmations, Bytes64> {
        let mut mutated_private_key = delegate_private_key.to_bytes();
        for byte in mutated_private_key.iter_mut() {
            *byte = byte.wrapping_add(1);
        }
        let mutated_delegate_private_key =
            DalekSigningKey::from_bytes(&mutated_private_key);
        let entity = Preconfirmations {
            expiration,
            preconfirmations: vec![],
        };
        let bytes = postcard::to_allocvec(&entity).unwrap();
        let typed_signature = mutated_delegate_private_key.sign(&bytes);
        let signature = Bytes64::new(typed_signature.to_bytes());
        Sealed { entity, signature }
    }

    fn protocol_key_pair() -> (SecretKey, PublicKey) {
        let secret_key = SecretKey::default();
        let public_key = secret_key.public_key();
        (secret_key, public_key)
    }

    fn delegate_key_pair() -> (DalekSigningKey, DalekVerifyingKey) {
        let secret_key = [99u8; 32];
        let secret_key = DalekSigningKey::from_bytes(&secret_key);
        let public_key = secret_key.verifying_key();
        (secret_key, public_key)
    }

    #[tokio::test]
    async fn add_new_delegate__includes_delegate_if_protocol_signature_verified() {
        // given
        let (secret_key, public_key) = protocol_key_pair();
        let mut adapter = PreconfirmationSignatureVerification::new(public_key);
        let valid_delegate_signature = arb_valid_sealed_delegate_signature(secret_key);

        // when
        let added = adapter.add_new_delegate(&valid_delegate_signature).await;

        // then
        assert!(added);
    }

    #[tokio::test]
    async fn check_preconfirmation_signature__can_verify_valid_pre_confirmations_signature_for_known_key(
    ) {
        // given
        let (protocol_secret_key, protocol_public_key) = protocol_key_pair();
        let (delegate_secret_key, delegate_public_key) = delegate_key_pair();
        let mut adapter = PreconfirmationSignatureVerification::new(protocol_public_key);
        let expiration = Tai64(u64::MAX);
        let valid_delegate_signature = valid_sealed_delegate_signature(
            protocol_secret_key,
            delegate_public_key,
            expiration,
        );
        let valid_pre_confirmation_signature =
            valid_pre_confirmation_signature(delegate_secret_key, expiration);
        let _ = adapter.add_new_delegate(&valid_delegate_signature).await;

        // when
        let verified = adapter
            .check_preconfirmation_signature(&valid_pre_confirmation_signature)
            .await;

        // then
        assert!(verified);
    }

    #[tokio::test]
    async fn check_preconfirmation_signature__will_not_verify_pre_confirmations_signature_for_unknown_delegate_key(
    ) {
        // given
        let (_, protocol_public_key) = protocol_key_pair();
        let (delegate_secret_key, _) = delegate_key_pair();
        let mut adapter = PreconfirmationSignatureVerification::new(protocol_public_key);
        let expiration = Tai64(100u64);
        let valid_pre_confirmation_signature =
            valid_pre_confirmation_signature(delegate_secret_key, expiration);

        // when
        let verified = adapter
            .check_preconfirmation_signature(&valid_pre_confirmation_signature)
            .await;

        // then
        assert!(!verified);
    }

    #[tokio::test]
    async fn check_preconfirmation_signature__will_not_verify_pre_confirmations_signature_with_invalid_signature(
    ) {
        // given
        let (protocol_secret_key, protocol_public_key) = protocol_key_pair();
        let (delegate_secret_key, delegate_public_key) = delegate_key_pair();
        let mut adapter = PreconfirmationSignatureVerification::new(protocol_public_key);
        let expiration = Tai64(100u64);
        let valid_delegate_signature = valid_sealed_delegate_signature(
            protocol_secret_key,
            delegate_public_key,
            expiration,
        );
        let invalid_pre_confirmation_signature =
            bad_pre_confirmation_signature(delegate_secret_key, expiration);
        let _ = adapter.add_new_delegate(&valid_delegate_signature).await;

        // when
        let verified = adapter
            .check_preconfirmation_signature(&invalid_pre_confirmation_signature)
            .await;

        // then
        assert!(!verified);
    }

    #[tokio::test]
    async fn check_preconfirmation_signature__will_not_verify_pre_confirmations_signature_expired_delegate_key(
    ) {
        // given
        let (protocol_secret_key, protocol_public_key) = protocol_key_pair();
        let (delegate_secret_key, delegate_public_key) = delegate_key_pair();
        let mut adapter = PreconfirmationSignatureVerification::new(protocol_public_key);
        let expiration = Tai64::now();
        let valid_delegate_signature = valid_sealed_delegate_signature(
            protocol_secret_key,
            delegate_public_key,
            expiration,
        );
        let valid_pre_confirmation_signature =
            valid_pre_confirmation_signature(delegate_secret_key, expiration);
        let _ = adapter.add_new_delegate(&valid_delegate_signature).await;

        // when
        // `Tai64` only has granularity of seconds, so we need to wait for the next second
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let verified = adapter
            .check_preconfirmation_signature(&valid_pre_confirmation_signature)
            .await;

        // then
        assert!(!verified);
    }

    #[tokio::test]
    async fn check_preconfirmation_signature__can_track_multipl_delegate_keys() {
        // given
        let (protocol_secret_key, protocol_public_key) = protocol_key_pair();
        let (delegate_secret_key, delegate_public_key) = delegate_key_pair();
        let mut adapter = PreconfirmationSignatureVerification::new(protocol_public_key);
        let first_expiration = Tai64(u64::MAX - 200);
        for expiration_modifier in 0..100u64 {
            let expiration = first_expiration + expiration_modifier;
            let valid_delegate_signature = valid_sealed_delegate_signature(
                protocol_secret_key,
                delegate_public_key,
                expiration,
            );
            let _ = adapter.add_new_delegate(&valid_delegate_signature).await;
        }
        let valid_pre_confirmation_signature =
            valid_pre_confirmation_signature(delegate_secret_key, first_expiration);

        // when
        let verified = adapter
            .check_preconfirmation_signature(&valid_pre_confirmation_signature)
            .await;

        // then
        assert!(verified);
    }
}
