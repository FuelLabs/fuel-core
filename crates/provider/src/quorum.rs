use crate::test_helpers::provider::MockProvider;
use alloy_provider::transport::{TransportError, TransportErrorKind};
use alloy_provider::Provider;
use async_trait::async_trait;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Default, Copy, Clone)]
pub enum Quorum {
    ///  The quorum is reached when all providers return the exact value
    All,
    /// The quorum is reached when the majority of the providers have returned a
    /// matching value, taking into account their weight.
    #[default]
    Majority,
    /// The quorum is reached when the cumulative weight of a matching return
    /// exceeds the given percentage of the total weight.
    ///
    /// NOTE: this must be less than `100u8`
    Percentage(u8),
    /// The quorum is reached when the given number of provider agree
    /// The configured weight is ignored in this case.
    ProviderCount(usize),
    /// The quorum is reached once the accumulated weight of the matching return
    /// exceeds this weight.
    Weight(u64),
}

impl Quorum {
    fn weight<T>(self, providers: &[WeightedProvider<T>]) -> u64 {
        match self {
            Quorum::All => providers.iter().map(|p| p.weight).sum::<u64>(),
            Quorum::Majority => {
                let total = providers.iter().map(|p| p.weight).sum::<u64>();
                let rem = total % 2;
                total / 2 + rem
            }
            Quorum::Percentage(p) => {
                providers.iter().map(|p| p.weight).sum::<u64>() * (p as u64) / 100
            }
            Quorum::ProviderCount(num) => {
                // take the lowest `num` weights
                let mut weights = providers.iter().map(|p| p.weight).collect::<Vec<_>>();
                weights.sort_unstable();
                weights.into_iter().take(num).sum()
            }
            Quorum::Weight(w) => w,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuorumProvider<T = Box<dyn Provider>> {
    quorum: Quorum,
    providers: Vec<WeightedProvider<T>>,
    quorum_weight: u64,
}

#[derive(Debug, Clone)]
pub struct WeightedProvider<T> {
    inner: T,
    weight: u64,
}

impl<T> WeightedProvider<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, weight: 1 }
    }

    pub fn with_weight(inner: T, weight: u64) -> Self {
        Self { inner, weight }
    }
}

#[derive(Debug, Clone)]
pub struct QuorumProviderBuilder<T> {
    quorum: Quorum,
    providers: Vec<WeightedProvider<T>>,
}

impl<T> Default for QuorumProviderBuilder<T> {
    fn default() -> Self {
        Self {
            quorum: Default::default(),
            providers: Vec::new(),
        }
    }
}

impl<T> QuorumProviderBuilder<T> {
    pub fn add_provider(mut self, provider: WeightedProvider<T>) -> Self {
        self.providers.push(provider);
        self
    }
    pub fn add_providers(
        mut self,
        providers: impl IntoIterator<Item=WeightedProvider<T>>,
    ) -> Self {
        for provider in providers {
            self.providers.push(provider);
        }
        self
    }

    /// Set the kind of quorum
    pub fn quorum(mut self, quorum: Quorum) -> Self {
        self.quorum = quorum;
        self
    }

    pub fn build(self) -> QuorumProvider<T> {
        let quorum_weight = self.quorum.weight(&self.providers);
        QuorumProvider {
            quorum: self.quorum,
            quorum_weight,
            providers: self.providers,
        }
    }
}

#[async_trait]
pub trait FuelProvider: Send + Sync {
    async fn get_block_number(&self) -> Result<u64, TransportError>;
}

#[async_trait]
impl FuelProvider for QuorumProvider<Box<dyn Provider>> {
    async fn get_block_number(&self) -> Result<u64, TransportError> {
        let futures: Vec<_> = self.providers.iter().map(|p| p.inner.get_block_number()).collect();
        let results = futures::future::join_all(futures).await;
        let agreement_map = self.calculate_agreement_weight(&results);
        let agreed_weight = self.quorum_weight;
        if let Some(best_value) = agreement_map
            .iter()
            .filter(|&(_, &weight)| weight >= agreed_weight)
            .max_by_key(|&(_, &weight)| weight)
            .map(|(&best_value, _)| best_value)
        {
            Ok(*best_value)
        } else {
            Err(TransportErrorKind::custom_str("Quorum not met".into()))
        }
    }
}

impl<T> QuorumProvider<T> {
    /// Convenience method for creating a `QuorumProviderBuilder` with same `JsonRpcClient` types
    pub fn builder() -> QuorumProviderBuilder<T> {
        QuorumProviderBuilder::default()
    }

    /// Instantiate a new `QuorumProvider` from a [`Quorum`] and a set of
    /// providers
    pub fn new(
        quorum: Quorum,
        providers: impl IntoIterator<Item=WeightedProvider<T>>,
    ) -> Self {
        Self::builder()
            .add_providers(providers)
            .quorum(quorum)
            .build()
    }

    /// Return a reference to the weighted providers
    pub fn providers(&self) -> &[WeightedProvider<T>] {
        &self.providers
    }

    /// The weight at which the provider reached a quorum
    pub fn quorum_weight(&self) -> u64 {
        self.quorum_weight
    }

    /// Add a provider to the set
    pub fn add_provider(&mut self, provider: WeightedProvider<T>) {
        self.providers.push(provider);
        self.quorum_weight = self.quorum.weight(&self.providers)
    }

    // Calculate total weight of providers that agree on a value
    fn calculate_agreement_weight<'a, V: Eq + std::hash::Hash>(
        &self,
        results: &'a [Result<V, TransportError>],
    ) -> HashMap<&'a V, u64> {
        let mut agreement_map = HashMap::new();

        for (i, result) in results.iter().enumerate() {
            if let Ok(value) = result {
                *agreement_map.entry(value).or_insert(0) += self.providers[i].weight;
            }
        }

        agreement_map
    }
}

#[derive(Error, Debug)]
/// Error thrown when sending an HTTP request
pub enum QuorumError {
    #[error("No Quorum reached.")]
    /// NoQuorumReached
    NoQuorumReached {
        /// Returned responses
        //values: Vec<Value>,
        /// Returned errors
        errors: Vec<TransportError>,
    },
}

#[cfg(test)]

mod tests {
    use super::*;
    use crate::test_helpers::provider::MockProvider;
    use alloy_provider::mock::Asserter;
    use alloy_provider::Provider;
    #[test]
    fn khar() {
        assert_eq!(1, 1);
    }

    #[tokio::test]
    async fn test_quorum() {
        let num = 5u64;
        let value = 42u64;
        let mut providers: Vec<WeightedProvider<Box<dyn Provider>>> = Vec::new();

        for _ in 0..num {
            let asserter = Asserter::new();
            asserter.push_success(&value);
            let mock = MockProvider::new(asserter);
            providers.push(WeightedProvider::new(Box::new(mock)));
        }

        let quorum = QuorumProvider::builder().add_providers(providers).quorum(Quorum::Majority).build();

        let blk = quorum.get_block_number().await.unwrap();
        assert_eq!(blk, value);
    }
}