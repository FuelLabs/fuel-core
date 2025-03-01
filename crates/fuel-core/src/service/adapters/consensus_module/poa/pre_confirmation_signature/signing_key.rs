use fuel_core_poa::pre_confirmation_signature_service::signing_key::SigningKey;

/// TODO: Decide what key to use for signing
/// <https://github.com/FuelLabs/fuel-core/issues/2782>
#[derive(Clone)]
pub struct DummyKey;

#[derive(Clone)]
pub struct DummyKeySignature<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl SigningKey for DummyKey {
    type Signature<T: Send + Clone> = DummyKeySignature<T>;

    fn sign<T: Send + Clone>(
        &self,
        _data: T,
    ) -> fuel_core_poa::pre_confirmation_signature_service::error::Result<
        Self::Signature<T>,
    > {
        todo!()
    }
}
