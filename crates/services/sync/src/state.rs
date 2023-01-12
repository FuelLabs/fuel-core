#![allow(clippy::type_complexity)]
use std::{
    marker::PhantomData,
    ops::RangeInclusive,
};

use fuel_core_types::blockchain::primitives::BlockHeight;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct State {
    in_flight: InFlight,
    seen: Seen,
    executed: Executed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum InFlight {
    Empty(Empty<InFlight>),
    Processing(Processing),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Seen {
    Empty(Empty<Seen>),
    Proposed(Proposed),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Executed {
    Empty(Empty<Executed>),
    Committed(Committed),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct Empty<T>(PhantomData<T>);

impl<T> Empty<T> {
    fn new() -> Self {
        Self(PhantomData)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Processing(RangeInclusive<u32>);

impl Processing {
    fn inc(self) -> Processing {
        let r = self.0;
        Self((*r.start() + 1)..=*r.end())
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for Processing {
    fn default() -> Self {
        Self(0..=0)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct Proposed(u32);

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct Committed(u32);

struct Tracker<I, S, E> {
    in_flight: I,
    seen: S,
    executed: E,
}

enum Either<A, B> {
    A(A),
    B(B),
}

impl State {
    pub fn new_empty() -> Self {
        Self {
            in_flight: InFlight::Empty(Empty::new()),
            seen: Seen::Empty(Empty::new()),
            executed: Executed::Empty(Empty::new()),
        }
    }

    pub fn new(executed: BlockHeight) -> Self {
        Self {
            in_flight: InFlight::Empty(Empty::new()),
            seen: Seen::Empty(Empty::new()),
            executed: Executed::Committed(Committed(*executed)),
        }
    }

    #[cfg(test)]
    pub fn test(
        in_flight: Option<RangeInclusive<u32>>,
        seen: Option<u32>,
        executed: Option<u32>,
    ) -> Self {
        Self {
            in_flight: in_flight.map_or(InFlight::Empty(Empty::new()), |r| {
                InFlight::Processing(Processing(r))
            }),
            seen: seen.map_or(Seen::Empty(Empty::new()), |r| Seen::Proposed(Proposed(r))),
            executed: executed.map_or(Executed::Empty(Empty::new()), |r| {
                Executed::Committed(Committed(r))
            }),
        }
    }

    pub fn see(&mut self, height: u32) -> bool {
        match self.clone() {
            State {
                in_flight,
                seen: Seen::Proposed(seen),
                executed,
            } => {
                let new_state: Self = seen.see(height, in_flight, executed);
                let change = *self != new_state;
                *self = new_state;
                change
            }
            State {
                in_flight,
                seen: Seen::Empty(seen),
                executed: Executed::Empty(executed),
            } => {
                let new_state: Self = seen.see(height, in_flight, executed);
                let change = *self != new_state;
                *self = new_state;
                change
            }
            _ => false,
        }
    }

    pub fn process(&mut self) {
        if let State {
            in_flight: InFlight::Empty(in_flight),
            seen: Seen::Proposed(seen),
            executed,
        } = self.clone()
        {
            *self = executed.process(in_flight, seen);
        }
    }

    pub fn execute_and_commit(&mut self) {
        if let State {
            in_flight: InFlight::Processing(in_flight),
            seen: Seen::Proposed(seen),
            executed,
        } = self.clone()
        {
            *self = executed.execute_and_commit(in_flight, seen);
        }
    }

    pub fn process_range(&self) -> Option<RangeInclusive<u32>> {
        match &self.in_flight {
            InFlight::Processing(Processing(r)) => Some(r.clone()),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn committed_height(&self) -> Option<BlockHeight> {
        match &self.executed {
            Executed::Committed(c) => Some(BlockHeight::from(c.0)),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn proposed_height(&self) -> Option<BlockHeight> {
        match &self.seen {
            Seen::Proposed(c) => Some(BlockHeight::from(c.0)),
            _ => None,
        }
    }
}

impl<I> Tracker<I, Empty<Seen>, Empty<Executed>> {
    fn see(self, height: u32) -> Tracker<I, Proposed, Empty<Executed>> {
        Tracker {
            in_flight: self.in_flight,
            seen: Proposed(height),
            executed: self.executed,
        }
    }
}

impl<I> Tracker<I, Proposed, Committed> {
    fn see(self, height: u32) -> Tracker<I, Proposed, Committed> {
        Tracker {
            in_flight: self.in_flight,
            seen: Proposed(self.seen.0.max(self.executed.0).max(height)),
            executed: self.executed,
        }
    }
}

impl<I> Tracker<I, Proposed, Empty<Executed>> {
    fn see(self, height: u32) -> Tracker<I, Proposed, Empty<Executed>> {
        Tracker {
            in_flight: self.in_flight,
            seen: Proposed(self.seen.0.max(height)),
            executed: self.executed,
        }
    }
}

impl Tracker<Empty<InFlight>, Proposed, Empty<Executed>> {
    fn process(self) -> Tracker<Processing, Proposed, Empty<Executed>> {
        Tracker {
            in_flight: Processing(0..=(self.seen.0)),
            seen: self.seen,
            executed: self.executed,
        }
    }
}

impl Tracker<Empty<InFlight>, Proposed, Committed> {
    fn process(self) -> Tracker<Processing, Proposed, Committed> {
        Tracker {
            in_flight: Processing((self.executed.0 + 1)..=(self.seen.0)),
            seen: self.seen,
            executed: self.executed,
        }
    }
}

impl<E> Tracker<Processing, Proposed, E> {
    fn execute_and_commit(
        self,
    ) -> Either<
        Tracker<Empty<InFlight>, Proposed, Committed>,
        Tracker<Processing, Proposed, Committed>,
    > {
        let Self {
            in_flight, seen, ..
        } = self;
        let executed = Committed(*in_flight.0.start());
        let in_flight = in_flight.inc();

        if in_flight.is_empty() {
            Either::A(Tracker {
                in_flight: Empty::<InFlight>::new(),
                seen,
                executed,
            })
        } else {
            Either::B(Tracker {
                in_flight,
                seen,
                executed,
            })
        }
    }
}

impl Proposed {
    fn see<I>(self, height: u32, in_flight: I, executed: Executed) -> State
    where
        I: Into<InFlight>,
    {
        match executed {
            Executed::Empty(executed) => Tracker {
                in_flight,
                seen: self,
                executed,
            }
            .see(height)
            .into(),
            Executed::Committed(executed) => Tracker {
                in_flight,
                seen: self,
                executed,
            }
            .see(height)
            .into(),
        }
    }
}

impl Empty<Seen> {
    fn see<I>(self, height: u32, in_flight: I, executed: Empty<Executed>) -> State
    where
        I: Into<InFlight>,
    {
        Tracker {
            in_flight,
            seen: self,
            executed,
        }
        .see(height)
        .into()
    }
}

impl Executed {
    fn process(self, in_flight: Empty<InFlight>, seen: Proposed) -> State {
        match self {
            Executed::Empty(executed) => Tracker {
                in_flight,
                seen,
                executed,
            }
            .process()
            .into(),
            Executed::Committed(executed) => Tracker {
                in_flight,
                seen,
                executed,
            }
            .process()
            .into(),
        }
    }
}

impl Executed {
    fn execute_and_commit(self, in_flight: Processing, seen: Proposed) -> State {
        Tracker {
            in_flight,
            seen,
            executed: self,
        }
        .execute_and_commit()
        .into()
    }
}

macro_rules! impl_from {
    ($from:ty, $to:ty, $v:ident) => {
        impl From<$from> for $to {
            fn from(t: $from) -> Self {
                use $to::*;
                $v(t)
            }
        }
    };
}

impl_from!(Empty<InFlight>, InFlight, Empty);
impl_from!(Processing, InFlight, Processing);

impl_from!(Empty<Seen>, Seen, Empty);
impl_from!(Proposed, Seen, Proposed);

impl_from!(Empty<Executed>, Executed, Empty);
impl_from!(Committed, Executed, Committed);

impl<I, S, E> From<Tracker<I, S, E>> for State
where
    I: Into<InFlight>,
    S: Into<Seen>,
    E: Into<Executed>,
{
    fn from(t: Tracker<I, S, E>) -> Self {
        Self {
            in_flight: t.in_flight.into(),
            seen: t.seen.into(),
            executed: t.executed.into(),
        }
    }
}

impl<I, S, E, IB, SB, EB> From<Either<Tracker<I, S, E>, Tracker<IB, SB, EB>>> for State
where
    I: Into<InFlight>,
    S: Into<Seen>,
    E: Into<Executed>,
    IB: Into<InFlight>,
    SB: Into<Seen>,
    EB: Into<Executed>,
{
    fn from(e: Either<Tracker<I, S, E>, Tracker<IB, SB, EB>>) -> Self {
        match e {
            Either::A(t) => t.into(),
            Either::B(t) => t.into(),
        }
    }
}
