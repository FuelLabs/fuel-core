use crate::service::PageSizer;

pub struct IdentityPageSizer {
    page_size: u64,
}

impl IdentityPageSizer {
    pub fn new(page_size: u64) -> Self {
        Self { page_size }
    }
}

impl PageSizer for IdentityPageSizer {
    fn update(&mut self, _downloaded_logs_count: u64, _rpc_error: bool) {
        // No-op: always returns the same page size
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}
