use anyhow::{anyhow, bail};
use graph::cheap_clone::CheapClone;
use graph::endpoint::EndpointMetrics;
use graph::firehose::{AvailableCapacity, SubgraphLimit};
use graph::prelude::rand::seq::IteratorRandom;
use graph::prelude::rand::{self, Rng};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

pub use graph::impl_slog_value;
use graph::prelude::Error;

use crate::adapter::EthereumAdapter as _;
use crate::capabilities::NodeCapabilities;
use crate::EthereumAdapter;

pub const DEFAULT_ADAPTER_ERROR_RETEST_PERCENT: f64 = 0.2;

#[derive(Debug, Clone)]
pub struct EthereumNetworkAdapter {
    endpoint_metrics: Arc<EndpointMetrics>,
    pub capabilities: NodeCapabilities,
    adapter: Arc<EthereumAdapter>,
    /// The maximum number of times this adapter can be used. We use the
    /// strong_count on `adapter` to determine whether the adapter is above
    /// that limit. That's a somewhat imprecise but convenient way to
    /// determine the number of connections
    limit: SubgraphLimit,
}

impl EthereumNetworkAdapter {
    fn is_call_only(&self) -> bool {
        self.adapter.is_call_only()
    }

    pub fn get_capacity(&self) -> AvailableCapacity {
        self.limit.get_capacity(Arc::strong_count(&self.adapter))
    }

    pub fn current_error_count(&self) -> u64 {
        self.endpoint_metrics.get_count(&self.provider().into())
    }
    pub fn provider(&self) -> &str {
        self.adapter.provider()
    }
}

#[derive(Debug, Clone)]
pub struct EthereumNetworkAdapters {
    pub adapters: Vec<EthereumNetworkAdapter>,
    call_only_adapters: Vec<EthereumNetworkAdapter>,
    // Percentage of request that should be used to retest errored adapters.
    retest_percent: f64,
}

impl Default for EthereumNetworkAdapters {
    fn default() -> Self {
        Self::new(None)
    }
}

impl EthereumNetworkAdapters {
    pub fn new(retest_percent: Option<f64>) -> Self {
        Self {
            adapters: vec![],
            call_only_adapters: vec![],
            retest_percent: retest_percent.unwrap_or(DEFAULT_ADAPTER_ERROR_RETEST_PERCENT),
        }
    }

    pub fn push_adapter(&mut self, adapter: EthereumNetworkAdapter) {
        if adapter.is_call_only() {
            self.call_only_adapters.push(adapter);
        } else {
            self.adapters.push(adapter);
        }
    }
    pub fn all_cheapest_with(
        &self,
        required_capabilities: &NodeCapabilities,
    ) -> impl Iterator<Item = &EthereumNetworkAdapter> + '_ {
        let cheapest_sufficient_capability = self
            .adapters
            .iter()
            .find(|adapter| &adapter.capabilities >= required_capabilities)
            .map(|adapter| &adapter.capabilities);

        self.adapters
            .iter()
            .filter(move |adapter| Some(&adapter.capabilities) == cheapest_sufficient_capability)
            .filter(|adapter| adapter.get_capacity() > AvailableCapacity::Unavailable)
    }

    pub fn cheapest_with(
        &self,
        required_capabilities: &NodeCapabilities,
    ) -> Result<Arc<EthereumAdapter>, Error> {
        let retest_rng: f64 = (&mut rand::thread_rng()).gen();
        let cheapest = self
            .all_cheapest_with(required_capabilities)
            .choose_multiple(&mut rand::thread_rng(), 3);
        let cheapest = cheapest.iter();

        // If request falls below the retest threshold, use this request to try and
        // reset the failed adapter. If a request succeeds the adapter will be more
        // likely to be selected afterwards.
        if retest_rng < self.retest_percent {
            cheapest.max_by_key(|adapter| adapter.current_error_count())
        } else {
            // The assumption here is that most RPC endpoints will not have limits
            // which makes the check for low/high available capacity less relevant.
            // So we essentially assume if it had available capacity when calling
            // `all_cheapest_with` then it prolly maintains that state and so we
            // just select whichever adapter is working better according to
            // the number of errors.
            cheapest.min_by_key(|adapter| adapter.current_error_count())
        }
        .map(|adapter| adapter.adapter.clone())
        .ok_or(anyhow!(
            "A matching Ethereum network with {:?} was not found.",
            required_capabilities
        ))
    }

    pub fn cheapest(&self) -> Option<Arc<EthereumAdapter>> {
        // EthereumAdapters are sorted by their NodeCapabilities when the EthereumNetworks
        // struct is instantiated so they do not need to be sorted here
        self.adapters
            .first()
            .map(|ethereum_network_adapter| ethereum_network_adapter.adapter.clone())
    }

    pub fn remove(&mut self, provider: &str) {
        self.adapters
            .retain(|adapter| adapter.adapter.provider() != provider);
    }

    pub fn call_or_cheapest(
        &self,
        capabilities: Option<&NodeCapabilities>,
    ) -> anyhow::Result<Arc<EthereumAdapter>> {
        // call_only_adapter can fail if we're out of capcity, this is fine since
        // we would want to fallback onto a full adapter
        // so we will ignore this error and return whatever comes out of `cheapest_with`
        match self.call_only_adapter() {
            Ok(Some(adapter)) => Ok(adapter),
            _ => self.cheapest_with(capabilities.unwrap_or(&NodeCapabilities {
                // Archive is required for call_only
                archive: true,
                traces: false,
            })),
        }
    }

    pub fn call_only_adapter(&self) -> anyhow::Result<Option<Arc<EthereumAdapter>>> {
        if self.call_only_adapters.is_empty() {
            return Ok(None);
        }

        let adapters = self
            .call_only_adapters
            .iter()
            .min_by_key(|x| Arc::strong_count(&x.adapter))
            .ok_or(anyhow!("no available call only endpoints"))?;

        // TODO: This will probably blow up a lot sooner than [limit] amount of
        // subgraphs, since we probably use a few instances.
        if !adapters
            .limit
            .has_capacity(Arc::strong_count(&adapters.adapter))
        {
            bail!("call only adapter has reached the concurrency limit");
        }

        // Cloning here ensure we have the correct count at any given time, if we return a reference it can be cloned later
        // which could cause a high number of endpoints to be given away before accounting for them.
        Ok(Some(adapters.adapter.clone()))
    }
}

#[derive(Clone)]
pub struct EthereumNetworks {
    pub metrics: Arc<EndpointMetrics>,
    pub networks: HashMap<String, EthereumNetworkAdapters>,
}

impl EthereumNetworks {
    pub fn new(metrics: Arc<EndpointMetrics>) -> EthereumNetworks {
        EthereumNetworks {
            networks: HashMap::new(),
            metrics,
        }
    }

    pub fn insert_empty(&mut self, name: String) {
        self.networks.entry(name).or_default();
    }

    pub fn insert(
        &mut self,
        name: String,
        capabilities: NodeCapabilities,
        adapter: Arc<EthereumAdapter>,
        limit: SubgraphLimit,
    ) {
        let network_adapters = self.networks.entry(name).or_default();

        network_adapters.push_adapter(EthereumNetworkAdapter {
            capabilities,
            adapter,
            limit,
            endpoint_metrics: self.metrics.cheap_clone(),
        });
    }

    pub fn remove(&mut self, name: &str, provider: &str) {
        if let Some(adapters) = self.networks.get_mut(name) {
            adapters.remove(provider);
        }
    }

    pub fn extend(&mut self, other_networks: EthereumNetworks) {
        self.networks.extend(other_networks.networks);
    }

    pub fn flatten(&self) -> Vec<(String, NodeCapabilities, Arc<EthereumAdapter>)> {
        self.networks
            .iter()
            .flat_map(|(network_name, network_adapters)| {
                network_adapters
                    .adapters
                    .iter()
                    .map(move |network_adapter| {
                        (
                            network_name.clone(),
                            network_adapter.capabilities,
                            network_adapter.adapter.clone(),
                        )
                    })
            })
            .collect()
    }

    pub fn sort(&mut self) {
        for adapters in self.networks.values_mut() {
            adapters.adapters.sort_by(|a, b| {
                a.capabilities
                    .partial_cmp(&b.capabilities)
                    // We can't define a total ordering over node capabilities,
                    // so incomparable items are considered equal and end up
                    // near each other.
                    .unwrap_or(Ordering::Equal)
            })
        }
    }

    pub fn adapter_with_capabilities(
        &self,
        network_name: String,
        requirements: &NodeCapabilities,
    ) -> Result<Arc<EthereumAdapter>, Error> {
        self.networks
            .get(&network_name)
            .ok_or(anyhow!("network not supported: {}", &network_name))
            .and_then(|adapters| adapters.cheapest_with(requirements))
    }
}

#[cfg(test)]
mod tests {
    use graph::{
        endpoint::{EndpointMetrics, Provider},
        firehose::SubgraphLimit,
        prelude::MetricsRegistry,
        slog::{o, Discard, Logger},
        tokio,
        url::Url,
    };
    use http::HeaderMap;
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::{
        EthereumAdapter, EthereumAdapterTrait, EthereumNetworks, ProviderEthRpcMetrics, Transport,
    };

    use super::{EthereumNetworkAdapter, EthereumNetworkAdapters, NodeCapabilities};

    #[test]
    fn ethereum_capabilities_comparison() {
        let archive = NodeCapabilities {
            archive: true,
            traces: false,
        };
        let traces = NodeCapabilities {
            archive: false,
            traces: true,
        };
        let archive_traces = NodeCapabilities {
            archive: true,
            traces: true,
        };
        let full = NodeCapabilities {
            archive: false,
            traces: false,
        };
        let full_traces = NodeCapabilities {
            archive: false,
            traces: true,
        };

        // Test all real combinations of capability comparisons
        assert_eq!(false, &full >= &archive);
        assert_eq!(false, &full >= &traces);
        assert_eq!(false, &full >= &archive_traces);
        assert_eq!(true, &full >= &full);
        assert_eq!(false, &full >= &full_traces);

        assert_eq!(true, &archive >= &archive);
        assert_eq!(false, &archive >= &traces);
        assert_eq!(false, &archive >= &archive_traces);
        assert_eq!(true, &archive >= &full);
        assert_eq!(false, &archive >= &full_traces);

        assert_eq!(false, &traces >= &archive);
        assert_eq!(true, &traces >= &traces);
        assert_eq!(false, &traces >= &archive_traces);
        assert_eq!(true, &traces >= &full);
        assert_eq!(true, &traces >= &full_traces);

        assert_eq!(true, &archive_traces >= &archive);
        assert_eq!(true, &archive_traces >= &traces);
        assert_eq!(true, &archive_traces >= &archive_traces);
        assert_eq!(true, &archive_traces >= &full);
        assert_eq!(true, &archive_traces >= &full_traces);

        assert_eq!(false, &full_traces >= &archive);
        assert_eq!(true, &full_traces >= &traces);
        assert_eq!(false, &full_traces >= &archive_traces);
        assert_eq!(true, &full_traces >= &full);
        assert_eq!(true, &full_traces >= &full_traces);
    }

    #[tokio::test]
    async fn adapter_selector_selects_eth_call() {
        let metrics = Arc::new(EndpointMetrics::mock());
        let chain = "mainnet".to_string();
        let logger = graph::log::logger(true);
        let mock_registry = Arc::new(MetricsRegistry::mock());
        let transport = Transport::new_rpc(
            Url::parse("http://127.0.0.1").unwrap(),
            HeaderMap::new(),
            metrics.clone(),
            "",
        );
        let provider_metrics = Arc::new(ProviderEthRpcMetrics::new(mock_registry.clone()));

        let eth_call_adapter = Arc::new(
            EthereumAdapter::new(
                logger.clone(),
                String::new(),
                transport.clone(),
                provider_metrics.clone(),
                true,
                true,
            )
            .await,
        );

        let eth_adapter = Arc::new(
            EthereumAdapter::new(
                logger.clone(),
                String::new(),
                transport.clone(),
                provider_metrics.clone(),
                true,
                false,
            )
            .await,
        );

        let mut adapters = {
            let mut ethereum_networks = EthereumNetworks::new(metrics);
            ethereum_networks.insert(
                chain.clone(),
                NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                eth_call_adapter.clone(),
                SubgraphLimit::Limit(3),
            );
            ethereum_networks.insert(
                chain.clone(),
                NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                eth_adapter.clone(),
                SubgraphLimit::Limit(3),
            );
            ethereum_networks.networks.get(&chain).unwrap().clone()
        };
        // one reference above and one inside adapters struct
        assert_eq!(Arc::strong_count(&eth_call_adapter), 2);
        assert_eq!(Arc::strong_count(&eth_adapter), 2);

        {
            // Not Found
            assert!(adapters
                .cheapest_with(&NodeCapabilities {
                    archive: false,
                    traces: true,
                })
                .is_err());

            // Check cheapest is not call only
            let adapter = adapters
                .cheapest_with(&NodeCapabilities {
                    archive: true,
                    traces: false,
                })
                .unwrap();
            assert_eq!(adapter.is_call_only(), false);
        }

        // Check limits
        {
            let adapter = adapters.call_or_cheapest(None).unwrap();
            assert!(adapter.is_call_only());
            assert_eq!(
                adapters.call_or_cheapest(None).unwrap().is_call_only(),
                false
            );
        }

        // Check empty falls back to call only
        {
            adapters.call_only_adapters = vec![];
            let adapter = adapters
                .call_or_cheapest(Some(&NodeCapabilities {
                    archive: true,
                    traces: false,
                }))
                .unwrap();
            assert_eq!(adapter.is_call_only(), false);
        }
    }

    #[tokio::test]
    async fn adapter_selector_unlimited() {
        let metrics = Arc::new(EndpointMetrics::mock());
        let chain = "mainnet".to_string();
        let logger = graph::log::logger(true);
        let mock_registry = Arc::new(MetricsRegistry::mock());
        let transport = Transport::new_rpc(
            Url::parse("http://127.0.0.1").unwrap(),
            HeaderMap::new(),
            metrics.clone(),
            "",
        );
        let provider_metrics = Arc::new(ProviderEthRpcMetrics::new(mock_registry.clone()));

        let eth_call_adapter = Arc::new(
            EthereumAdapter::new(
                logger.clone(),
                String::new(),
                transport.clone(),
                provider_metrics.clone(),
                true,
                true,
            )
            .await,
        );

        let eth_adapter = Arc::new(
            EthereumAdapter::new(
                logger.clone(),
                String::new(),
                transport.clone(),
                provider_metrics.clone(),
                true,
                false,
            )
            .await,
        );

        let adapters = {
            let mut ethereum_networks = EthereumNetworks::new(metrics);
            ethereum_networks.insert(
                chain.clone(),
                NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                eth_call_adapter.clone(),
                SubgraphLimit::Unlimited,
            );
            ethereum_networks.insert(
                chain.clone(),
                NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                eth_adapter.clone(),
                SubgraphLimit::Limit(3),
            );
            ethereum_networks.networks.get(&chain).unwrap().clone()
        };
        // one reference above and one inside adapters struct
        assert_eq!(Arc::strong_count(&eth_call_adapter), 2);
        assert_eq!(Arc::strong_count(&eth_adapter), 2);

        let keep: Vec<Arc<EthereumAdapter>> = vec![0; 10]
            .iter()
            .map(|_| adapters.call_or_cheapest(None).unwrap())
            .collect();
        assert_eq!(keep.iter().any(|a| !a.is_call_only()), false);
    }

    #[tokio::test]
    async fn adapter_selector_disable_call_only_fallback() {
        let metrics = Arc::new(EndpointMetrics::mock());
        let chain = "mainnet".to_string();
        let logger = graph::log::logger(true);
        let mock_registry = Arc::new(MetricsRegistry::mock());
        let transport = Transport::new_rpc(
            Url::parse("http://127.0.0.1").unwrap(),
            HeaderMap::new(),
            metrics.clone(),
            "",
        );
        let provider_metrics = Arc::new(ProviderEthRpcMetrics::new(mock_registry.clone()));

        let eth_call_adapter = Arc::new(
            EthereumAdapter::new(
                logger.clone(),
                String::new(),
                transport.clone(),
                provider_metrics.clone(),
                true,
                true,
            )
            .await,
        );

        let eth_adapter = Arc::new(
            EthereumAdapter::new(
                logger.clone(),
                String::new(),
                transport.clone(),
                provider_metrics.clone(),
                true,
                false,
            )
            .await,
        );

        let adapters = {
            let mut ethereum_networks = EthereumNetworks::new(metrics);
            ethereum_networks.insert(
                chain.clone(),
                NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                eth_call_adapter.clone(),
                SubgraphLimit::Disabled,
            );
            ethereum_networks.insert(
                chain.clone(),
                NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                eth_adapter.clone(),
                SubgraphLimit::Limit(3),
            );
            ethereum_networks.networks.get(&chain).unwrap().clone()
        };
        // one reference above and one inside adapters struct
        assert_eq!(Arc::strong_count(&eth_call_adapter), 2);
        assert_eq!(Arc::strong_count(&eth_adapter), 2);
        assert_eq!(
            adapters.call_or_cheapest(None).unwrap().is_call_only(),
            false
        );
    }

    #[tokio::test]
    async fn adapter_selector_no_call_only_fallback() {
        let metrics = Arc::new(EndpointMetrics::mock());
        let chain = "mainnet".to_string();
        let logger = graph::log::logger(true);
        let mock_registry = Arc::new(MetricsRegistry::mock());
        let transport = Transport::new_rpc(
            Url::parse("http://127.0.0.1").unwrap(),
            HeaderMap::new(),
            metrics.clone(),
            "",
        );
        let provider_metrics = Arc::new(ProviderEthRpcMetrics::new(mock_registry.clone()));

        let eth_adapter = Arc::new(
            EthereumAdapter::new(
                logger.clone(),
                String::new(),
                transport.clone(),
                provider_metrics.clone(),
                true,
                false,
            )
            .await,
        );

        let adapters = {
            let mut ethereum_networks = EthereumNetworks::new(metrics);
            ethereum_networks.insert(
                chain.clone(),
                NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                eth_adapter.clone(),
                SubgraphLimit::Limit(3),
            );
            ethereum_networks.networks.get(&chain).unwrap().clone()
        };
        // one reference above and one inside adapters struct
        assert_eq!(Arc::strong_count(&eth_adapter), 2);
        assert_eq!(
            adapters.call_or_cheapest(None).unwrap().is_call_only(),
            false
        );
    }

    #[tokio::test]
    async fn eth_adapter_selection_multiple_adapters() {
        let logger = Logger::root(Discard, o!());
        let unavailable_provider = Uuid::new_v4().to_string();
        let error_provider = Uuid::new_v4().to_string();
        let no_error_provider = Uuid::new_v4().to_string();

        let mock_registry = Arc::new(MetricsRegistry::mock());
        let metrics = Arc::new(EndpointMetrics::new(
            logger,
            &[
                unavailable_provider.clone(),
                error_provider.clone(),
                no_error_provider.clone(),
            ],
            mock_registry.clone(),
        ));
        let logger = graph::log::logger(true);
        let provider_metrics = Arc::new(ProviderEthRpcMetrics::new(mock_registry.clone()));

        let adapters = vec![
            fake_adapter(
                &logger,
                &unavailable_provider,
                &provider_metrics,
                &metrics,
                false,
            )
            .await,
            fake_adapter(&logger, &error_provider, &provider_metrics, &metrics, false).await,
            fake_adapter(
                &logger,
                &no_error_provider,
                &provider_metrics,
                &metrics,
                false,
            )
            .await,
        ];

        // Set errors
        metrics.report_for_test(&Provider::from(error_provider.clone()), false);

        let mut no_retest_adapters = EthereumNetworkAdapters::new(Some(0f64));
        let mut always_retest_adapters = EthereumNetworkAdapters::new(Some(1f64));
        adapters.iter().cloned().for_each(|adapter| {
            let limit = if adapter.provider() == unavailable_provider {
                SubgraphLimit::Disabled
            } else {
                SubgraphLimit::Unlimited
            };

            no_retest_adapters.adapters.push(EthereumNetworkAdapter {
                endpoint_metrics: metrics.clone(),
                capabilities: NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                adapter: adapter.clone(),
                limit: limit.clone(),
            });
            always_retest_adapters
                .adapters
                .push(EthereumNetworkAdapter {
                    endpoint_metrics: metrics.clone(),
                    capabilities: NodeCapabilities {
                        archive: true,
                        traces: false,
                    },
                    adapter,
                    limit,
                });
        });

        assert_eq!(
            no_retest_adapters
                .cheapest_with(&NodeCapabilities {
                    archive: true,
                    traces: false,
                })
                .unwrap()
                .provider(),
            no_error_provider
        );
        assert_eq!(
            always_retest_adapters
                .cheapest_with(&NodeCapabilities {
                    archive: true,
                    traces: false,
                })
                .unwrap()
                .provider(),
            error_provider
        );
    }

    #[tokio::test]
    async fn eth_adapter_selection_single_adapter() {
        let logger = Logger::root(Discard, o!());
        let unavailable_provider = Uuid::new_v4().to_string();
        let error_provider = Uuid::new_v4().to_string();
        let no_error_provider = Uuid::new_v4().to_string();

        let mock_registry = Arc::new(MetricsRegistry::mock());
        let metrics = Arc::new(EndpointMetrics::new(
            logger,
            &[
                unavailable_provider,
                error_provider.clone(),
                no_error_provider.clone(),
            ],
            mock_registry.clone(),
        ));
        let logger = graph::log::logger(true);
        let provider_metrics = Arc::new(ProviderEthRpcMetrics::new(mock_registry.clone()));

        // Set errors
        metrics.report_for_test(&Provider::from(error_provider.clone()), false);

        let mut no_retest_adapters = EthereumNetworkAdapters::new(Some(0f64));
        no_retest_adapters.adapters.push(EthereumNetworkAdapter {
            endpoint_metrics: metrics.clone(),
            capabilities: NodeCapabilities {
                archive: true,
                traces: false,
            },
            adapter: fake_adapter(&logger, &error_provider, &provider_metrics, &metrics, false)
                .await,
            limit: SubgraphLimit::Unlimited,
        });
        assert_eq!(
            no_retest_adapters
                .cheapest_with(&NodeCapabilities {
                    archive: true,
                    traces: false,
                })
                .unwrap()
                .provider(),
            error_provider
        );

        let mut always_retest_adapters = EthereumNetworkAdapters::new(Some(1f64));
        always_retest_adapters
            .adapters
            .push(EthereumNetworkAdapter {
                endpoint_metrics: metrics.clone(),
                capabilities: NodeCapabilities {
                    archive: true,
                    traces: false,
                },
                adapter: fake_adapter(
                    &logger,
                    &no_error_provider,
                    &provider_metrics,
                    &metrics,
                    false,
                )
                .await,
                limit: SubgraphLimit::Unlimited,
            });
        assert_eq!(
            always_retest_adapters
                .cheapest_with(&NodeCapabilities {
                    archive: true,
                    traces: false,
                })
                .unwrap()
                .provider(),
            no_error_provider
        );

        let mut no_available_adapter = EthereumNetworkAdapters::default();
        no_available_adapter.adapters.push(EthereumNetworkAdapter {
            endpoint_metrics: metrics.clone(),
            capabilities: NodeCapabilities {
                archive: true,
                traces: false,
            },
            adapter: fake_adapter(
                &logger,
                &no_error_provider,
                &provider_metrics,
                &metrics,
                false,
            )
            .await,
            limit: SubgraphLimit::Disabled,
        });
        let res = no_available_adapter.cheapest_with(&NodeCapabilities {
            archive: true,
            traces: false,
        });
        assert!(res.is_err(), "{:?}", res);
    }

    async fn fake_adapter(
        logger: &Logger,
        provider: &str,
        provider_metrics: &Arc<ProviderEthRpcMetrics>,
        endpoint_metrics: &Arc<EndpointMetrics>,
        call_only: bool,
    ) -> Arc<EthereumAdapter> {
        let transport = Transport::new_rpc(
            Url::parse(&"http://127.0.0.1").unwrap(),
            HeaderMap::new(),
            endpoint_metrics.clone(),
            "",
        );

        Arc::new(
            EthereumAdapter::new(
                logger.clone(),
                provider.to_string(),
                transport.clone(),
                provider_metrics.clone(),
                true,
                call_only,
            )
            .await,
        )
    }
}
