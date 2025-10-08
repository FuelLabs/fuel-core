use crate::service::{
    PageSizer,
    RpcOutcome,
};

pub struct IdentityPageSizer {
    page_size: u64,
}

impl IdentityPageSizer {
    pub fn new(page_size: u64) -> Self {
        Self { page_size }
    }
}

impl PageSizer for IdentityPageSizer {
    fn update(&mut self, _: RpcOutcome) {
        // No-op: always returns the same page size
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}
