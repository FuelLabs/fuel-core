#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Count {
    pub headers: usize,
    pub transactions: usize,
    pub consensus: usize,
    pub executes: usize,
    pub blocks: usize,
}

impl Count {
    pub fn inc_headers(&mut self) {
        self.headers += 1;
    }

    pub fn inc_transactions(&mut self) {
        self.transactions += 1;
    }

    pub fn inc_consensus(&mut self) {
        self.consensus += 1;
    }

    pub fn inc_executes(&mut self) {
        self.executes += 1;
    }

    pub fn inc_blocks(&mut self) {
        self.blocks += 1;
    }
}
