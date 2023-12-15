use graph::prelude::DeploymentHash;
use graph::schema::InputSchema;
use std::env;
use std::fs;
use std::process::exit;

pub fn usage(msg: &str) -> ! {
    println!("{}", msg);
    println!("usage: schema schema.graphql");
    println!("\nPrint the API schema we derive from the given input schema");
    std::process::exit(1);
}

pub fn ensure<T, E: std::fmt::Display>(res: Result<T, E>, msg: &str) -> T {
    match res {
        Ok(ok) => ok,
        Err(err) => {
            eprintln!("{}:\n    {}", msg, err);
            exit(1)
        }
    }
}

pub fn main() {
    let args: Vec<String> = env::args().collect();
    let schema = match args.len() {
        0 | 1 => usage("please provide a GraphQL schema"),
        2 => args[1].clone(),
        _ => usage("too many arguments"),
    };
    let schema = ensure(fs::read_to_string(schema), "Can not read schema file");
    let id = DeploymentHash::new("unknown").unwrap();
    let schema = ensure(InputSchema::parse(&schema, id), "Failed to parse schema");
    let schema = ensure(schema.api_schema(), "Failed to convert to API schema");

    println!("{}", schema.schema().document);
}
