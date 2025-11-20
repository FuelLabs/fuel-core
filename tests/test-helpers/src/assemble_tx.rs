use fuel_core_client::client::{
    FuelClient,
    types::{
        TransactionStatus,
        assemble_tx::{
            Account,
            ChangePolicy,
            Predicate,
            RequiredBalance,
        },
    },
};
use fuel_core_types::{
    fuel_asm::Instruction,
    fuel_crypto::SecretKey,
    fuel_tx::{
        Address,
        AssetId,
        Input,
        Output,
        Signable,
        Transaction,
        TransactionBuilder,
    },
};
use std::{
    collections::HashMap,
    future::Future,
    io,
};

pub trait AssembleAndRunTx {
    fn assemble_and_run_tx(
        &self,
        tx_to_assemble: &Transaction,
        wallet: SigningAccount,
    ) -> impl Future<Output = io::Result<TransactionStatus>> + Send;

    fn assemble_transaction(
        &self,
        tx_to_assemble: &Transaction,
        wallet: SigningAccount,
        required_balances: Vec<RequiredBalance>,
    ) -> impl Future<Output = io::Result<Transaction>> + Send;

    fn run_transfer(
        &self,
        wallet: SigningAccount,
        recipients: Vec<(Address, AssetId, u64)>,
    ) -> impl Future<Output = io::Result<TransactionStatus>> + Send;

    fn assemble_transfer(
        &self,
        wallet: SigningAccount,
        recipients: Vec<(Address, AssetId, u64)>,
    ) -> impl Future<Output = io::Result<Transaction>> + Send;

    fn run_script(
        &self,
        script: Vec<Instruction>,
        script_data: Vec<u8>,
        wallet: SigningAccount,
    ) -> impl Future<Output = io::Result<TransactionStatus>> + Send;

    fn assemble_script(
        &self,
        script: Vec<Instruction>,
        script_data: Vec<u8>,
        wallet: SigningAccount,
    ) -> impl Future<Output = io::Result<Transaction>> + Send;
}

impl AssembleAndRunTx for FuelClient {
    async fn assemble_and_run_tx(
        &self,
        tx_to_assemble: &Transaction,
        wallet: SigningAccount,
    ) -> io::Result<TransactionStatus> {
        let tx = self
            .assemble_transaction(tx_to_assemble, wallet, Vec::new())
            .await?;

        self.submit_and_await_commit(&tx).await
    }

    async fn assemble_transaction(
        &self,
        tx_to_assemble: &Transaction,
        wallet: SigningAccount,
        mut required_balances: Vec<RequiredBalance>,
    ) -> io::Result<Transaction> {
        let params = self.chain_info().await?.consensus_parameters;
        let chain_id = params.chain_id();
        let base_asset_id = *params.base_asset_id();
        let wallet_owner = wallet.owner();

        let mut fee_payer_index = required_balances
            .iter()
            .enumerate()
            .find(|(_, balance)| balance.account.owner() == wallet_owner)
            .map(|(i, _)| i);

        if fee_payer_index.is_none() {
            let required_balance = match &wallet {
                SigningAccount::Wallet(_) => RequiredBalance {
                    asset_id: base_asset_id,
                    amount: 0,
                    account: Account::Address(wallet_owner),
                    change_policy: ChangePolicy::Change(wallet_owner),
                },
                SigningAccount::Predicate {
                    predicate,
                    predicate_data,
                } => RequiredBalance {
                    asset_id: base_asset_id,
                    amount: 0,
                    account: Account::Predicate(Predicate {
                        address: wallet_owner,
                        predicate: predicate.clone(),
                        predicate_data: predicate_data.clone(),
                    }),
                    change_policy: ChangePolicy::Change(wallet_owner),
                },
            };

            fee_payer_index = Some(required_balances.len());
            required_balances.push(required_balance);
        }

        let fee_index =
            u16::try_from(fee_payer_index.expect("Fee index set above; qed")).unwrap();
        let mut tx = self
            .assemble_tx(
                tx_to_assemble,
                1,
                required_balances,
                fee_index,
                None,
                true,
                None,
            )
            .await?;

        match wallet {
            SigningAccount::Wallet(secret_key) => {
                match &mut tx.transaction {
                    Transaction::Script(tx) => tx.sign_inputs(&secret_key, &chain_id),
                    Transaction::Create(tx) => tx.sign_inputs(&secret_key, &chain_id),
                    Transaction::Mint(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Cannot sign mint transaction",
                        ));
                    }
                    Transaction::Upgrade(tx) => tx.sign_inputs(&secret_key, &chain_id),
                    Transaction::Upload(tx) => tx.sign_inputs(&secret_key, &chain_id),
                    Transaction::Blob(tx) => tx.sign_inputs(&secret_key, &chain_id),
                };
            }
            SigningAccount::Predicate { .. } => {
                // Do nothing
            }
        }

        Ok(tx.transaction)
    }

    async fn run_transfer(
        &self,
        wallet: SigningAccount,
        recipients: Vec<(Address, AssetId, u64)>,
    ) -> io::Result<TransactionStatus> {
        let tx = self.assemble_transfer(wallet, recipients).await?;

        self.submit_and_await_commit(&tx).await
    }

    async fn assemble_transfer(
        &self,
        wallet: SigningAccount,
        recipients: Vec<(Address, AssetId, u64)>,
    ) -> io::Result<Transaction> {
        let wallet_owner = wallet.owner();

        let mut tx_to_assemble = TransactionBuilder::script(vec![], vec![]);

        let mut total_balances = HashMap::new();

        for (recipient, asset_id, amount) in recipients {
            tx_to_assemble.add_output(Output::Coin {
                to: recipient,
                asset_id,
                amount,
            });

            total_balances
                .entry(asset_id)
                .and_modify(|balance| *balance += amount)
                .or_insert(amount);
        }

        let required_balances = total_balances
            .into_iter()
            .map(|(asset_id, amount)| RequiredBalance {
                asset_id,
                amount,
                account: wallet.clone().into_account(),
                change_policy: ChangePolicy::Change(wallet_owner),
            })
            .collect();

        let tx = tx_to_assemble.finalize_as_transaction();

        self.assemble_transaction(&tx, wallet, required_balances)
            .await
    }

    async fn run_script(
        &self,
        script: Vec<Instruction>,
        script_data: Vec<u8>,
        wallet: SigningAccount,
    ) -> io::Result<TransactionStatus> {
        let tx = self.assemble_script(script, script_data, wallet).await?;

        self.submit_and_await_commit(&tx).await
    }

    async fn assemble_script(
        &self,
        script: Vec<Instruction>,
        script_data: Vec<u8>,
        wallet: SigningAccount,
    ) -> io::Result<Transaction> {
        let tx_to_assemble =
            TransactionBuilder::script(script.into_iter().collect(), script_data)
                .finalize_as_transaction();

        self.assemble_transaction(&tx_to_assemble, wallet, vec![])
            .await
    }
}

#[derive(Clone)]
pub enum SigningAccount {
    Wallet(SecretKey),
    Predicate {
        predicate: Vec<u8>,
        predicate_data: Vec<u8>,
    },
}

impl SigningAccount {
    pub fn owner(&self) -> Address {
        match self {
            SigningAccount::Wallet(secret_key) => Input::owner(&secret_key.public_key()),
            SigningAccount::Predicate { predicate, .. } => {
                Input::predicate_owner(predicate)
            }
        }
    }

    pub fn into_account(self) -> Account {
        let wallet_owner = self.owner();
        match self {
            SigningAccount::Wallet(_) => Account::Address(wallet_owner),
            SigningAccount::Predicate {
                predicate,
                predicate_data,
            } => Account::Predicate(Predicate {
                address: wallet_owner,
                predicate: predicate.clone(),
                predicate_data: predicate_data.clone(),
            }),
        }
    }
}
