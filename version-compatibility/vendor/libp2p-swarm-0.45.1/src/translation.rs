// Copyright 2019 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use libp2p_core::{multiaddr::Protocol, Multiaddr};

/// Perform IP address translation.
///
/// Given an `original` [`Multiaddr`] and some `observed` [`Multiaddr`], replace the first protocol
/// of the `original` with the first protocol of the `observed` [`Multiaddr`] and return this
/// translated [`Multiaddr`].
///
/// This function can for example be useful when handling tcp connections. Tcp does not listen and
/// dial on the same port by default. Thus when receiving an observed address on a connection that
/// we initiated, it will contain our dialing port, not our listening port. We need to take the ip
/// address or dns address from the observed address and the port from the original address.
///
/// This is a mixed-mode translation, i.e. an IPv4 / DNS4 address may be replaced by an IPv6 / DNS6
/// address and vice versa.
///
/// If the first [`Protocol`]s are not IP addresses, `None` is returned instead.
#[doc(hidden)]
pub fn _address_translation(original: &Multiaddr, observed: &Multiaddr) -> Option<Multiaddr> {
    original.replace(0, move |proto| match proto {
        Protocol::Ip4(_)
        | Protocol::Ip6(_)
        | Protocol::Dns(_)
        | Protocol::Dns4(_)
        | Protocol::Dns6(_) => match observed.iter().next() {
            x @ Some(Protocol::Ip4(_)) => x,
            x @ Some(Protocol::Ip6(_)) => x,
            x @ Some(Protocol::Dns(_)) => x,
            x @ Some(Protocol::Dns4(_)) => x,
            x @ Some(Protocol::Dns6(_)) => x,
            _ => None,
        },
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_translation() {
        struct Test {
            original: Multiaddr,
            observed: Multiaddr,
            expected: Multiaddr,
        }

        let tests = [
            // Basic ipv4.
            Test {
                original: "/ip4/192.0.2.1/tcp/1".parse().unwrap(),
                observed: "/ip4/192.0.2.2/tcp/2".parse().unwrap(),
                expected: "/ip4/192.0.2.2/tcp/1".parse().unwrap(),
            },
            // Basic ipv6.
            Test {
                original: "/ip6/2001:db8:0:0:0:0:0:0/tcp/1".parse().unwrap(),
                observed: "/ip6/2001:db8:0:0:0:0:0:1/tcp/2".parse().unwrap(),
                expected: "/ip6/2001:db8:0:0:0:0:0:1/tcp/1".parse().unwrap(),
            },
            // Ipv4  ipv6 mix.
            Test {
                original: "/ip4/192.0.2.1/tcp/1".parse().unwrap(),
                observed: "/ip6/2001:db8:0:0:0:0:0:1/tcp/2".parse().unwrap(),
                expected: "/ip6/2001:db8:0:0:0:0:0:1/tcp/1".parse().unwrap(),
            },
            // Ipv6  ipv4 mix.
            Test {
                original: "/ip6/2001:db8:0:0:0:0:0:0/tcp/1".parse().unwrap(),
                observed: "/ip4/192.0.2.2/tcp/2".parse().unwrap(),
                expected: "/ip4/192.0.2.2/tcp/1".parse().unwrap(),
            },
            // Dns.
            Test {
                original: "/dns4/foo/tcp/1".parse().unwrap(),
                observed: "/dns4/bar/tcp/2".parse().unwrap(),
                expected: "/dns4/bar/tcp/1".parse().unwrap(),
            },
            // Ipv4 Dns mix.
            Test {
                original: "/ip4/192.0.2.1/tcp/1".parse().unwrap(),
                observed: "/dns4/bar/tcp/2".parse().unwrap(),
                expected: "/dns4/bar/tcp/1".parse().unwrap(),
            },
        ];

        for test in tests.iter() {
            assert_eq!(
                _address_translation(&test.original, &test.observed),
                Some(test.expected.clone())
            );
        }
    }
}
