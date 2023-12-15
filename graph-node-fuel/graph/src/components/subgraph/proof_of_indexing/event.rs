use crate::components::subgraph::Entity;
use crate::prelude::impl_slog_value;
use stable_hash_legacy::StableHasher;
use std::collections::BTreeMap;
use std::fmt;
use strum_macros::IntoStaticStr;

#[derive(IntoStaticStr)]
pub enum ProofOfIndexingEvent<'a> {
    /// For when an entity is removed from the store.
    RemoveEntity { entity_type: &'a str, id: &'a str },
    /// For when an entity is set into the store.
    SetEntity {
        entity_type: &'a str,
        id: &'a str,
        data: &'a Entity,
    },
    /// For when a deterministic error has happened.
    ///
    /// The number of redacted events covers the events previous to this one
    /// which are no longer transacted to the database. The property that we
    /// want to maintain is that no two distinct databases share the same PoI.
    /// Since there is no event for the beginning of a handler the
    /// non-fatal-errors feature creates an ambiguity without this field. This
    /// is best illustrated by example. Consider:
    ///    1. Start handler
    ///       1. Save Entity A
    ///    2. Start handler
    ///       2. Save Entity B
    ///       3. Save Entity C
    ///       4. Deterministic Error
    ///
    /// The Deterministic Error redacts the effect of 2.1 and 2.2 since entity B
    /// and C are not saved to the database.
    ///
    /// Without the redacted events field, this results in the following event
    /// stream for the PoI: [Save(A), Save(B), Save(C), DeterministicError]
    ///
    /// But, an equivalent PoI would be generated with this sequence of events:
    ///    1. Start handler
    ///       1. Save Entity A
    ///       2. Save Entity B
    ///    2. Start handler
    ///       1. Save Entity C
    ///       2. Deterministic Error
    ///
    /// The databases would be different even though the PoI is the same. (The
    /// first database in [A] and the second is [A, B])
    ///
    /// By emitting the number of redacted events we get a different PoI for
    /// different databases because the PoIs become:
    ///
    ///   [Save(A), Save(B), Save(C), DeterministicError(2)]
    ///
    ///   [Save(A), Save(B), Save(C), DeterministicError(1)]
    ///
    /// for the first and second cases respectively.
    DeterministicError { redacted_events: u64 },
}

impl stable_hash_legacy::StableHash for ProofOfIndexingEvent<'_> {
    fn stable_hash<H: StableHasher>(&self, mut sequence_number: H::Seq, state: &mut H) {
        use stable_hash_legacy::prelude::*;
        use ProofOfIndexingEvent::*;

        let str: &'static str = self.into();
        str.stable_hash(sequence_number.next_child(), state);
        match self {
            RemoveEntity { entity_type, id } => {
                entity_type.stable_hash(sequence_number.next_child(), state);
                id.stable_hash(sequence_number.next_child(), state);
            }
            SetEntity {
                entity_type,
                id,
                data,
            } => {
                entity_type.stable_hash(sequence_number.next_child(), state);
                id.stable_hash(sequence_number.next_child(), state);

                stable_hash_legacy::utils::AsUnorderedSet(*data)
                    .stable_hash(sequence_number.next_child(), state);
            }
            DeterministicError { redacted_events } => {
                redacted_events.stable_hash(sequence_number.next_child(), state)
            }
        }
    }
}

impl stable_hash::StableHash for ProofOfIndexingEvent<'_> {
    fn stable_hash<H: stable_hash::StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        use stable_hash::prelude::*;

        let variant = match self {
            Self::RemoveEntity { entity_type, id } => {
                entity_type.stable_hash(field_address.child(0), state);
                id.stable_hash(field_address.child(1), state);
                1
            }
            Self::SetEntity {
                entity_type,
                id,
                data,
            } => {
                entity_type.stable_hash(field_address.child(0), state);
                id.stable_hash(field_address.child(1), state);
                stable_hash::utils::AsUnorderedSet(*data)
                    .stable_hash::<_>(field_address.child(2), state);
                2
            }
            Self::DeterministicError { redacted_events } => {
                redacted_events.stable_hash(field_address.child(0), state);
                3
            }
        };

        state.write(field_address, &[variant]);
    }
}

/// Different than #[derive(Debug)] in order to be deterministic so logs can be
/// diffed easily. In particular, we swap out the HashMap for a BTreeMap when
/// printing the data field of the SetEntity variant so that the keys are
/// sorted.
impl fmt::Debug for ProofOfIndexingEvent<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = f.debug_struct(self.into());
        match self {
            Self::RemoveEntity { entity_type, id } => {
                builder.field("entity_type", entity_type);
                builder.field("id", id);
            }
            Self::SetEntity {
                entity_type,
                id,
                data,
            } => {
                builder.field("entity_type", entity_type);
                builder.field("id", id);
                builder.field(
                    "data",
                    &data
                        .sorted_ref()
                        .iter()
                        .cloned()
                        .collect::<BTreeMap<_, _>>(),
                );
            }
            Self::DeterministicError { redacted_events } => {
                builder.field("redacted_events", redacted_events);
            }
        }
        builder.finish()
    }
}

impl_slog_value!(ProofOfIndexingEvent<'_>, "{:?}");
