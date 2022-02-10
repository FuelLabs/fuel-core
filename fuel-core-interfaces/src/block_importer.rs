// TODO full block
#[derive(Clone, Debug)]
pub enum NewBlockEvent {
    NewBlockCreated(u64),
    NewBlockIncluded(u64),
}
