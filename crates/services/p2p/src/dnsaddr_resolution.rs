use anyhow::anyhow;
use hickory_resolver::TokioAsyncResolver;
use libp2p::Multiaddr;
use std::pin::Pin;

/// The prefix for `dnsaddr` protocol TXT record lookups.
const DNSADDR_PREFIX: &str = "_dnsaddr.";
/// The maximum number of DNS lookups when dialing.
/// This limit is for preventing malicious or misconfigured DNS records from causing infinite recursion.
const MAX_DNS_LOOKUPS: usize = 10;

pub(crate) struct DnsResolver {
    resolver: TokioAsyncResolver,
}

impl DnsResolver {
    pub(crate) async fn lookup_dnsaddr(
        &self,
        addr: &str,
    ) -> anyhow::Result<Vec<Multiaddr>> {
        self.resolve_recursive(addr, 0).await
    }

    pub(crate) async fn new() -> anyhow::Result<Box<Self>> {
        let resolver = TokioAsyncResolver::tokio_from_system_conf()?;
        Ok(Box::new(Self { resolver }))
    }

    /// Internal method to handle recursive DNS lookups.
    fn resolve_recursive<'a>(
        &'a self,
        addr: &'a str,
        depth: usize,
    ) -> Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<Vec<Multiaddr>>> + Send + 'a>,
    > {
        Box::pin(async move {
            if depth >= MAX_DNS_LOOKUPS {
                return Err(anyhow!("Maximum DNS lookup depth exceeded"));
            }

            let mut multiaddrs = vec![];
            let dns_lookup_url = format!("{}{}", DNSADDR_PREFIX, addr);
            let txt_records = self.resolver.txt_lookup(dns_lookup_url).await?;

            for record in txt_records {
                let parsed = record.to_string();
                if !parsed.starts_with("dnsaddr") {
                    continue;
                }

                let dnsaddr_value = parsed
                    .split("dnsaddr=")
                    .nth(1)
                    .ok_or_else(|| anyhow!("Invalid DNS address: {:?}", parsed))?;

                // Check if the parsed value is a multiaddress or another dnsaddr.
                if dnsaddr_value.starts_with("/dnsaddr") {
                    let nested_dnsaddr = dnsaddr_value
                        .split('/')
                        .nth(2)
                        .ok_or_else(|| anyhow!("Invalid nested dnsaddr"))?;
                    // Recursively resolve the nested dnsaddr
                    #[allow(clippy::arithmetic_side_effects)]
                    let nested_addrs =
                        self.resolve_recursive(nested_dnsaddr, depth + 1).await?;
                    multiaddrs.extend(nested_addrs);
                } else if let Ok(multiaddr) = dnsaddr_value.parse::<Multiaddr>() {
                    multiaddrs.push(multiaddr);
                }
            }

            Ok(multiaddrs)
        })
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[tokio::test]
    async fn dns_resolver__parses_all_multiaddresses_from_mainnet_dnsaddr_entry() {
        // given
        let resolver = DnsResolver::new().await.unwrap();
        let expected_multiaddrs: HashSet<Multiaddr> = [
            "/dns/p2p-mainnet.fuel.network/tcp/30336/p2p/16Uiu2HAkxjhwNYtwawWUexYn84MsrA9ivFWkNHmiF4hSieoNP7Jd",
            "/dns/p2p-mainnet.fuel.network/tcp/30337/p2p/16Uiu2HAmQunK6Dd81BXh3rW2ZsszgviPgGMuHw39vv2XxbkuCfaw",
            "/dns/p2p-mainnet.fuel.network/tcp/30333/p2p/16Uiu2HAkuiLZNrfecgDYHJZV5LoEtCXqqRCqHY3yLBqs4LQk8jJg",
            "/dns/p2p-mainnet.fuel.network/tcp/30334/p2p/16Uiu2HAkzYNa6yMykppS1ij69mKoKjrZEr11oHGiM5Mpc8nKjVDM",
            "/dns/p2p-mainnet.fuel.network/tcp/30335/p2p/16Uiu2HAm5yqpTv1QVk3SepUYzeKXTWMuE2VqMWHq5qQLPR2Udg6s"
        ].iter().map(|s| s.parse().unwrap()).collect();

        // when
        // run a `dig +short txt _dnsaddr.core-test.fuellabs.net` to get the TXT records
        let multiaddrs = resolver
            .lookup_dnsaddr("core-test.fuellabs.net")
            .await
            .unwrap();
        // then
        for multiaddr in multiaddrs.iter() {
            assert!(
                expected_multiaddrs.contains(multiaddr),
                "Unexpected multiaddr: {:?}",
                multiaddr
            );
        }
    }
}
