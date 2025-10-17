use alloy_provider::{
    Provider,
    RootProvider,
    network::Ethereum,
};

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
        providers: impl IntoIterator<Item = WeightedProvider<T>>,
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

impl<T> QuorumProvider<T> {
    /// Convenience method for creating a `QuorumProviderBuilder` with same `JsonRpcClient` types
    pub fn builder() -> QuorumProviderBuilder<T> {
        QuorumProviderBuilder::default()
    }

    /// Instantiate a new `QuorumProvider` from a [`Quorum`] and a set of
    /// providers
    pub fn new(
        quorum: Quorum,
        providers: impl IntoIterator<Item = WeightedProvider<T>>,
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
}

impl Provider for QuorumProvider<Box<dyn Provider>> {
    fn root(&self) -> &RootProvider<Ethereum> {
        // TODO: Fix me
        &self.providers.first().unwrap().inner.root()
    }
}
