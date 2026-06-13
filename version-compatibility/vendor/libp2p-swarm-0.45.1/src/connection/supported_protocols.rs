use crate::handler::ProtocolsChange;
use crate::StreamProtocol;
use std::collections::HashSet;

#[derive(Default, Clone, Debug)]
pub struct SupportedProtocols {
    protocols: HashSet<StreamProtocol>,
}

impl SupportedProtocols {
    pub fn on_protocols_change(&mut self, change: ProtocolsChange) -> bool {
        match change {
            ProtocolsChange::Added(added) => {
                let mut changed = false;

                for p in added {
                    changed |= self.protocols.insert(p.clone());
                }

                changed
            }
            ProtocolsChange::Removed(removed) => {
                let mut changed = false;

                for p in removed {
                    changed |= self.protocols.remove(p);
                }

                changed
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &StreamProtocol> {
        self.protocols.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::{ProtocolsAdded, ProtocolsRemoved};

    #[test]
    fn protocols_change_added_returns_correct_changed_value() {
        let mut protocols = SupportedProtocols::default();

        let changed = protocols.on_protocols_change(add_foo());
        assert!(changed);

        let changed = protocols.on_protocols_change(add_foo());
        assert!(!changed);

        let changed = protocols.on_protocols_change(add_foo_bar());
        assert!(changed);
    }

    #[test]
    fn protocols_change_removed_returns_correct_changed_value() {
        let mut protocols = SupportedProtocols::default();

        let changed = protocols.on_protocols_change(remove_foo());
        assert!(!changed);

        protocols.on_protocols_change(add_foo());

        let changed = protocols.on_protocols_change(remove_foo());
        assert!(changed);
    }

    fn add_foo() -> ProtocolsChange<'static> {
        ProtocolsChange::Added(ProtocolsAdded {
            protocols: FOO_PROTOCOLS.iter(),
        })
    }

    fn add_foo_bar() -> ProtocolsChange<'static> {
        ProtocolsChange::Added(ProtocolsAdded {
            protocols: FOO_BAR_PROTOCOLS.iter(),
        })
    }

    fn remove_foo() -> ProtocolsChange<'static> {
        ProtocolsChange::Removed(ProtocolsRemoved {
            protocols: FOO_PROTOCOLS.iter(),
        })
    }

    static FOO_PROTOCOLS: &[StreamProtocol] = &[StreamProtocol::new("/foo")];
    static FOO_BAR_PROTOCOLS: &[StreamProtocol] =
        &[StreamProtocol::new("/foo"), StreamProtocol::new("/bar")];
}
