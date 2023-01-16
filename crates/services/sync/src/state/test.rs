use super::*;
use test_case::test_case;

#[test_case(State::new(None, None) => Status::Uninitialized)]
#[test_case(State::new(10, None) => Status::Committed(10))]
#[test_case(State::new(None, 10) => Status::Processing(0..=10))]
#[test_case(State::new(10, 10) => Status::Committed(10))]
#[test_case(State::new(10, 11) => Status::Processing(11..=11))]
#[test_case(State::new(1, 10) => Status::Processing(2..=10))]
#[test_case(State::new(11, 10) => Status::Committed(11))]
fn test_new(state: State) -> Status {
    state.status
}

#[test_case(State::new(None, None), 0 => Status::Committed(0))]
#[test_case(State::new(0, None), 0 => Status::Committed(0))]
#[test_case(State::new(1, None), 0 => Status::Committed(1))]
#[test_case(State::new(2, None), 0 => Status::Processing(1..=1))]
#[test_case(State::new(20, None), 10 => Status::Processing(11..=19))]
#[test_case(State::new(0, None), 1 => Status::Committed(1))]
#[test_case(State::new(0, None), 2 => Status::Committed(2))]
#[test_case(State::new(None, 0), 0 => Status::Committed(0))]
#[test_case(State::new(None, 1), 0 => Status::Processing(1..=1))]
#[test_case(State::new(None, 2), 0 => Status::Processing(1..=2))]
#[test_case(State::new(None, 0), 1 => Status::Committed(1))]
#[test_case(State::new(None, 0), 2 => Status::Committed(2))]
#[test_case(State::new(0, 0), 0 => Status::Committed(0))]
#[test_case(State::new(0, 0), 1 => Status::Committed(1))]
#[test_case(State::new(0, 0), 2 => Status::Committed(2))]
#[test_case(State::new(0, 1), 0 => Status::Processing(1..=1))]
#[test_case(State::new(0, 2), 0 => Status::Processing(1..=2))]
#[test_case(State::new(0, 4), 2 => Status::Processing(3..=4))]
#[test_case(State::new(1, 0), 0 => Status::Committed(1))]
#[test_case(State::new(2, 0), 0 => Status::Processing(1..=1))]
#[test_case(State::new(2, 2), 2 => Status::Committed(2))]
fn test_commit(mut state: State, height: u32) -> Status {
    state.commit(height);
    state.status
}

#[test_case(0, 0 => None)]
#[test_case(0, 1 => None)]
#[test_case(0, 2 => None)]
#[test_case(1, 0 => None)]
#[test_case(2, 0 => Some(1..=1))]
#[test_case(30, 0 => Some(1..=29))]
fn test_creates_new_existing(existing: u32, commit: u32) -> Option<RangeInclusive<u32>> {
    commit_creates_processing(&existing, &commit)
}

#[test_case(State::new(None, None), 0 => Status::Processing(0..=0))]
#[test_case(State::new(0, None), 0 => Status::Committed(0))]
#[test_case(State::new(1, None), 0 => Status::Committed(1))]
#[test_case(State::new(2, None), 0 => Status::Committed(2))]
#[test_case(State::new(20, None), 10 => Status::Committed(20))]
#[test_case(State::new(10, None), 20 => Status::Processing(11..=20))]
#[test_case(State::new(0, None), 1 => Status::Processing(1..=1))]
#[test_case(State::new(0, None), 2 => Status::Processing(1..=2))]
#[test_case(State::new(None, 0), 0 => Status::Processing(0..=0))]
#[test_case(State::new(None, 0), 1 => Status::Processing(0..=1))]
#[test_case(State::new(None, 0), 2 => Status::Processing(0..=2))]
#[test_case(State::new(None, 1), 0 => Status::Processing(0..=1))]
#[test_case(State::new(None, 2), 0 => Status::Processing(0..=2))]
#[test_case(State::new(0, 0), 0 => Status::Committed(0))]
#[test_case(State::new(0, 0), 1 => Status::Processing(1..=1))]
#[test_case(State::new(0, 0), 2 => Status::Processing(1..=2))]
#[test_case(State::new(0, 1), 0 => Status::Processing(1..=1))]
#[test_case(State::new(0, 2), 0 => Status::Processing(1..=2))]
#[test_case(State::new(0, 4), 2 => Status::Processing(1..=4))]
#[test_case(State::new(1, 0), 0 => Status::Committed(1))]
#[test_case(State::new(2, 0), 0 => Status::Committed(2))]
#[test_case(State::new(2, 2), 2 => Status::Committed(2))]
fn test_observe(mut state: State, height: u32) -> Status {
    state.observe(height);
    state.status
}

#[test_case(State::new(None, None), 0..=0 => Status::Uninitialized)]
#[test_case(State::new(None, None), 0..=100 => Status::Uninitialized)]
#[test_case(State::new(0, None), 0..=0 => Status::Committed(0))]
#[test_case(State::new(0, None), 0..=100 => Status::Committed(0))]
#[test_case(State::new(0, None), 10..=100 => Status::Committed(0))]
#[test_case(State::new(1, None), 0..=100 => Status::Committed(1))]
#[test_case(State::new(2, None), 0..=100 => Status::Committed(2))]
#[test_case(State::new(10, 20), 10..=20 => Status::Committed(10))]
#[test_case(State::new(None, 20), 0..=20 => Status::Uninitialized)]
#[test_case(State::new(None, 20), 10..=20 => Status::Processing(0..=9))]
#[test_case(State::new(10, None), 20..=20 => Status::Committed(10))]
#[test_case(State::new(0, None), 0..=1 => Status::Committed(0))]
#[test_case(State::new(0, None), 0..=2 => Status::Committed(0))]
#[test_case(State::new(None, 0), 0..=1 => Status::Uninitialized)]
#[test_case(State::new(None, 0), 1..=1 => Status::Processing(0..=0))]
#[test_case(State::new(None, 1), 0..=1 => Status::Uninitialized)]
#[test_case(State::new(None, 1), 1..=1 => Status::Processing(0..=0))]
#[test_case(State::new(None, 2), 1..=1 => Status::Processing(0..=0))]
#[test_case(State::new(None, 2), 2..=2 => Status::Processing(0..=1))]
#[test_case(State::new(0, 1), 0..=0 => Status::Processing(1..=1))]
#[test_case(State::new(0, 1), 0..=1 => Status::Committed(0))]
#[test_case(State::new(0, 1), 0..=100 => Status::Committed(0))]
#[test_case(State::new(0, 2), 0..=0 => Status::Processing(1..=2))]
#[test_case(State::new(0, 2), 0..=1 => Status::Committed(0))]
#[test_case(State::new(0, 30), 0..=1 => Status::Committed(0))]
#[test_case(State::new(0, 30), 0..=3 => Status::Committed(0))]
#[test_case(State::new(10, 20), 0..=9 => Status::Processing(11..=20))]
#[test_case(State::new(10, 20), 0..=10 => Status::Processing(11..=20))]
#[test_case(State::new(10, 20), 0..=11 => Status::Committed(10))]
#[test_case(State::new(10, 20), 0..=19 => Status::Committed(10))]
#[test_case(State::new(10, 20), 0..=20 => Status::Committed(10))]
#[test_case(State::new(10, 20), 0..=21 => Status::Committed(10))]
#[test_case(State::new(10, 20), 0..=22 => Status::Committed(10))]
#[test_case(State::new(10, 20), 10..=19 => Status::Committed(10))]
#[test_case(State::new(10, 20), 11..=19 => Status::Committed(10))]
#[test_case(State::new(10, 20), 12..=19 => Status::Processing(11..=11))]
#[test_case(State::new(10, 20), 12..=20 => Status::Processing(11..=11))]
#[test_case(State::new(10, 20), 12..=21 => Status::Processing(11..=11))]
#[test_case(State::new(10, 20), 12..=22 => Status::Processing(11..=11))]
#[test_case(State::new(10, 20), 13..=22 => Status::Processing(11..=12))]
#[test_case(State::new(10, 20), 19..=22 => Status::Processing(11..=18))]
#[test_case(State::new(10, 20), 20..=22 => Status::Processing(11..=19))]
#[test_case(State::new(10, 20), 21..=22 => Status::Processing(11..=20))]
#[test_case(State::new(10, 20), 22..=22 => Status::Processing(11..=20))]
fn test_failed(mut state: State, range: RangeInclusive<u32>) -> Status {
    state.failed_to_process(range);
    state.status
}
