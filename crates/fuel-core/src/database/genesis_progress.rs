#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenesisProgress {
    coins: usize,
    messages: usize,
    contracts: usize,
    contract_states: usize,
    contract_balances: usize,
    contract_roots: usize,
}

impl GenesisProgress {
    pub fn new() -> Self {
        Self {
            coins: 0,
            messages: 0,
            contracts: 0,
            contract_states: 0,
            contract_balances: 0,
            contract_roots: 0,
        }
    }

    pub fn coins(&self) -> usize {
        self.coins
    }

    pub fn messages(&self) -> usize {
        self.messages
    }

    pub fn contracts(&self) -> usize {
        self.contracts
    }

    pub fn contract_states(&self) -> usize {
        self.contract_states
    }

    pub fn contract_balances(&self) -> usize {
        self.contract_balances
    }

    pub fn contract_roots(&self) -> usize {
        self.contract_roots
    }

    pub fn add_coin(&mut self) {
        self.coins.checked_add(1).expect("Maximum number of coins exceeded");
    }

    pub fn add_message(&mut self) {
        self.messages.checked_add(1).expect("Maximum number of messages exceeded");
    }

    pub fn add_contract(&mut self) {
        self.contracts.checked_add(1).expect("Maximum number of contracts exceeded");
    }

    pub fn add_contract_state(&mut self, batch_size: usize) {
        self.contract_states.checked_add(batch_size).expect("Maximum number of contract states exceeded");
    }

    pub fn add_balance(&mut self, batch_size: usize) {
        self.contract_balances.checked_add(batch_size).expect("Maximum number of contract balances exceeded");
    }

    pub fn add_contract_root(&mut self, batch_size: usize) {
        self.contract_roots.checked_add(batch_size).expect("Maximum number of contract roots exceeded");
    }
}
