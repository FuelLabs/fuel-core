use crate::client::schema::{schema, HexString256};

#[derive(cynic::FragmentArguments, Debug)]
pub struct AccountByAddressArgs {
    pub address: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "AccountByAddressArgs"
)]
pub struct AccountByAddressQuery {
    #[arguments(address = &args.address)]
    pub account: Option<Account>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Account {
    pub address: HexString256,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_by_address_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = AccountByAddressQuery::build(AccountByAddressArgs {
            address: HexString256::default(),
        });
        insta::assert_snapshot!(operation.query)
    }
}
