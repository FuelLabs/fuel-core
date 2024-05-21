//! Ports used by the relayer to access the outside world

pub trait SharedSequencerClient: Send + Sync {
    fn send(
        &mut self,
        signing_key: &Secret<SecretKeyWrapper>,
        block: SealedBlock,
    ) -> anyhow::Result<()>;
}
