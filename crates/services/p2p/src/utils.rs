use crate::Multiaddr;

pub fn is_dialable(multiaddr: &Multiaddr) -> bool {
    // Check if the multiaddr is dialable
    match multiaddr.protocol_stack().next() {
        None => false,
        Some(protocol) => protocol != "p2p",
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn is_dialable__returns_false_for_empty_multiaddress() {
        // given
        let multiaddr = Multiaddr::empty();

        // when
        let dialable = is_dialable(&multiaddr);

        // then
        assert!(!dialable);
    }

    #[test]
    fn is_dialable__kats_returns_false_for_multiaddress_without_location_multiaddress() {
        // given
        let multiaddr = Multiaddr::from_str(
            "/p2p/16Uiu2HAmUjL2n1rS3gxvr45G3BvNZTPLEYZc97H1kXX5G4u1XNEe",
        )
        .unwrap();

        // when
        let dialable = is_dialable(&multiaddr);

        // then
        assert!(!dialable);
    }

    #[test]
    fn is_dialable__returns_true_for_multiaddress_with_location_multiaddress() {
        // given
        let multiaddr = Multiaddr::from_str(
            "/ip4/0.0.0.0/p2p/16Uiu2HAmUjL2n1rS3gxvr45G3BvNZTPLEYZc97H1kXX5G4u1XNEe",
        )
        .unwrap();

        // when
        let dialable = is_dialable(&multiaddr);

        // then
        assert!(dialable);
    }
}
