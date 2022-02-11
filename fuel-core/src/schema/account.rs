use crate::schema::scalars::HexString256;
use async_graphql::{Context, Object};
use fuel_tx::Address;

pub struct Account(Address);

impl From<Address> for Account {
    fn from(address: Address) -> Self {
        Self(address)
    }
}

#[Object]
impl Account {
    async fn address(&self) -> HexString256 {
        self.0.into()
    }
}

#[derive(Default)]
pub struct AccountQuery;

#[Object]
impl AccountQuery {
    async fn account(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Address of the Account")] address: HexString256,
    ) -> async_graphql::Result<Option<Account>> {
        let account = Account(address.into());
        Ok(Some(account))
    }
}
