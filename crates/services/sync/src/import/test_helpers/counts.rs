use fuel_core_services::SharedMutex;

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Count {
    pub headers: usize,
    pub transactions: usize,
    pub consensus: usize,
    pub executes: usize,
    pub blocks: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub now: Count,
    pub max: Count,
}

pub type SharedCounts = SharedMutex<Counts>;

impl Counts {
    pub fn inc_headers(&mut self) {
        self.now.headers += 1;
        self.max.headers = self.max.headers.max(self.now.headers);
    }
    pub fn dec_headers(&mut self) {
        self.now.headers -= 1;
    }
    pub fn inc_transactions(&mut self) {
        self.now.transactions += 1;
        self.max.transactions = self.max.transactions.max(self.now.transactions);
    }
    pub fn dec_transactions(&mut self) {
        self.now.transactions -= 1;
    }
    pub fn inc_consensus(&mut self) {
        self.now.consensus += 1;
        self.max.consensus = self.max.consensus.max(self.now.consensus);
    }
    pub fn dec_consensus(&mut self) {
        self.now.consensus -= 1;
    }
    pub fn inc_executes(&mut self) {
        self.now.executes += 1;
        self.max.executes = self.max.executes.max(self.now.executes);
    }
    pub fn dec_executes(&mut self) {
        self.now.executes -= 1;
    }
    pub fn inc_blocks(&mut self) {
        self.now.blocks += 1;
        self.max.blocks = self.max.blocks.max(self.now.blocks);
    }
    pub fn dec_blocks(&mut self) {
        self.now.blocks -= 1;
    }
}
