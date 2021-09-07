mod schema {
    cynic::use_schema!("./assets/schema.sdl");
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct Health {
    pub health: bool,
}

#[derive(cynic::FragmentArguments)]
pub struct TxArg {
    pub tx: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "TxArg"
)]
pub struct Run {
    #[arguments(tx = &args.tx)]
    pub run: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Mutation")]
pub struct StartSession {
    pub start_session: cynic::Id,
}

#[derive(cynic::FragmentArguments)]
pub struct IdArg {
    pub id: cynic::Id,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "IdArg"
)]
pub struct EndSession {
    #[arguments(id = &args.id)]
    pub end_session: bool,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "IdArg"
)]
pub struct Reset {
    #[arguments(id = &args.id)]
    pub reset: bool,
}

#[derive(cynic::FragmentArguments)]
pub struct ExecuteArgs {
    pub id: cynic::Id,
    pub op: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "ExecuteArgs"
)]
pub struct Execute {
    #[arguments(id = &args.id, op = &args.op)]
    pub execute: bool,
}

#[derive(cynic::FragmentArguments)]
pub struct RegisterArgs {
    pub id: cynic::Id,
    pub register: i32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "RegisterArgs"
)]
pub struct Register {
    #[arguments(id = &args.id, register = &args.register)]
    pub register: i32,
}

#[derive(cynic::FragmentArguments)]
pub struct MemoryArgs {
    pub id: cynic::Id,
    pub start: i32,
    pub size: i32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "MemoryArgs"
)]
pub struct Memory {
    #[arguments(id = &args.id, start = &args.start, size = &args.size)]
    pub memory: String,
}
