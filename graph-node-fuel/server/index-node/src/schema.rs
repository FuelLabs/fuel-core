use graph::{
    prelude::*,
    schema::{ApiSchema, Schema},
};

lazy_static! {
    pub static ref SCHEMA: Arc<ApiSchema> = {
        let raw_schema = include_str!("./schema.graphql");
        let document = graphql_parser::parse_schema(raw_schema).unwrap();
        Arc::new(
            ApiSchema::from_graphql_schema(
                Schema::new(DeploymentHash::new("indexnode").unwrap(), document).unwrap(),
            )
            .unwrap(),
        )
    };
}

#[test]
fn schema_parses() {
    let _ = &*SCHEMA;
}
