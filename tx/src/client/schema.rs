mod schema {
    cynic::use_schema!("./assets/tx.sdl");
}

#[derive(cynic::FragmentArguments)]
pub struct TxArg {
    pub tx: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/tx.sdl",
    graphql_type = "MutationRoot",
    argument_struct = "TxArg"
)]
pub struct Run {
    #[arguments(tx = &args.tx)]
    pub run: String,
}
