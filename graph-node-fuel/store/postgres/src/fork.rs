use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Mutex,
};

use graph::{
    block_on,
    components::store::SubgraphFork as SubgraphForkTrait,
    constraint_violation,
    prelude::{
        info, r::Value as RValue, reqwest, serde_json, DeploymentHash, Entity, Logger, Serialize,
        StoreError, Value, ValueType,
    },
    schema::Field,
    url::Url,
};
use graph::{data::value::Word, schema::InputSchema};
use inflector::Inflector;

#[derive(Serialize, Debug, PartialEq)]
struct Query {
    query: String,
    variables: Variables,
}

#[derive(Serialize, Debug, PartialEq)]
struct Variables {
    id: String,
}

/// SubgraphFork represents a simple subgraph forking mechanism
/// which lazily fetches entities from a remote subgraph's store
/// associated with a GraphQL `endpoint`.
///
/// Since this mechanism is used for debug forks, entities are
/// fetched only once per id in order to avoid fetching an entity
/// that was deleted from the local store and thus causing inconsistencies.
pub(crate) struct SubgraphFork {
    client: reqwest::Client,
    endpoint: Url,
    schema: InputSchema,
    fetched_ids: Mutex<HashSet<String>>,
    logger: Logger,
}

impl SubgraphForkTrait for SubgraphFork {
    fn fetch(&self, entity_type_name: String, id: String) -> Result<Option<Entity>, StoreError> {
        {
            let mut fids = self.fetched_ids.lock().map_err(|e| {
                StoreError::ForkFailure(format!(
                    "attempt to acquire lock on `fetched_ids` failed with {}",
                    e,
                ))
            })?;
            if fids.contains(&id) {
                info!(self.logger, "Already fetched entity! Abort!"; "entity_type" => entity_type_name, "id" => id);
                return Ok(None);
            }
            fids.insert(id.clone());
        }

        info!(self.logger, "Fetching entity from {}", &self.endpoint; "entity_type" => &entity_type_name, "id" => &id);

        // NOTE: Subgraph fork compatibility checking (similar to the grafting compatibility checks)
        // will be added in the future (in a separate PR).
        // Currently, forking incompatible subgraphs is allowed, but, for example, storing the
        // incompatible fetched entities in the local store results in an error.
        let entity_type = self.schema.entity_type(&entity_type_name)?;
        let fields = &entity_type
            .object_type()
            .ok_or_else(|| {
                constraint_violation!("no object type called `{}` found", entity_type_name)
            })?
            .fields;

        let query = Query {
            query: self.query_string(&entity_type_name, fields)?,
            variables: Variables { id },
        };
        let raw_json = block_on(self.send(&query))?;
        if !raw_json.contains("data") {
            return Err(StoreError::ForkFailure(format!(
                "the GraphQL query \"{:?}\" to `{}` failed with \"{}\"",
                query, self.endpoint, raw_json,
            )));
        }

        let entity =
            SubgraphFork::extract_entity(&self.schema, &raw_json, &entity_type_name, fields)?;
        Ok(entity)
    }
}

impl SubgraphFork {
    pub(crate) fn new(
        base: Url,
        id: DeploymentHash,
        schema: InputSchema,
        logger: Logger,
    ) -> Result<Self, StoreError> {
        Ok(Self {
            client: reqwest::Client::new(),
            endpoint: base
                .join(id.as_str())
                .map_err(|e| StoreError::ForkFailure(format!("failed to join fork base: {}", e)))?,
            schema,
            fetched_ids: Mutex::new(HashSet::new()),
            logger,
        })
    }

    async fn send(&self, query: &Query) -> Result<String, StoreError> {
        let res = self
            .client
            .post(self.endpoint.clone())
            .json(query)
            .send()
            .await
            .map_err(|e| {
                StoreError::ForkFailure(format!(
                    "sending a GraphQL query to `{}` failed with: \"{}\"",
                    self.endpoint, e,
                ))
            })?
            .text()
            .await
            .map_err(|e| {
                StoreError::ForkFailure(format!(
                    "receiving a response from `{}` failed with: \"{}\"",
                    self.endpoint, e,
                ))
            })?;
        Ok(res)
    }

    fn query_string(&self, entity_type: &str, fields: &[Field]) -> Result<String, StoreError> {
        let names = fields
            .iter()
            .map(|f| {
                let fname = f.name.to_string();
                let ftype = f.field_type.to_string().replace(['!', '[', ']'], "");
                match ValueType::from_str(&ftype) {
                    Ok(_) => fname,
                    Err(_) => {
                        format!("{} {{ id }}", fname,)
                    }
                }
            })
            .collect::<Vec<String>>();

        Ok(format!(
            "\
query Query ($id: String) {{
    {}(id: $id, subgraphError: allow) {{
        {}
    }}
}}",
            entity_type.to_camel_case(),
            names.join(" ").trim(),
        ))
    }

    fn extract_entity(
        schema: &InputSchema,
        raw_json: &str,
        entity_type: &str,
        fields: &[Field],
    ) -> Result<Option<Entity>, StoreError> {
        let json: serde_json::Value = serde_json::from_str(raw_json).unwrap();
        let entity = &json["data"][entity_type.to_lowercase()];

        if entity.is_null() {
            return Ok(None);
        }

        let map: HashMap<Word, Value> = {
            let mut map = HashMap::new();
            for f in fields {
                if f.is_derived {
                    // Derived fields are not resolved, so it's safe to ignore them.
                    continue;
                }

                let value = entity.get(f.name.as_str()).unwrap().clone();
                let value = if let Some(id) = value.get("id") {
                    RValue::String(id.as_str().unwrap().to_string())
                } else if let Some(list) = value.as_array() {
                    RValue::List(
                        list.iter()
                            .map(|v| match v.get("id") {
                                Some(id) => RValue::String(id.as_str().unwrap().to_string()),
                                None => RValue::from(v.clone()),
                            })
                            .collect(),
                    )
                } else {
                    RValue::from(value)
                };

                let value = Value::from_query_value(&value, &f.field_type).map_err(|e| {
                    StoreError::ForkFailure(format!(
                        "Unexpected error during entity extraction! Failed to convert JSON value `{}` to type `{}`: {}",
                        value,
                        f.field_type,
                        e
                    ))
                })?;
                map.insert(Word::from(f.name.clone()), value);
            }
            map
        };

        Ok(Some(
            schema
                .make_entity(map)
                .map_err(|e| StoreError::EntityValidationError(e))?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    use graph::{
        data::store::scalar,
        prelude::{s::Type, DeploymentHash},
        slog::{self, o},
    };

    fn test_base() -> Url {
        Url::parse("https://api.thegraph.com/subgraph/id/").unwrap()
    }

    fn test_id() -> DeploymentHash {
        DeploymentHash::new("test").unwrap()
    }

    fn test_schema() -> InputSchema {
        InputSchema::parse(
            r#"type Gravatar @entity {
  id: ID!
  owner: Bytes!
  displayName: String!
  imageUrl: String!
}"#,
            DeploymentHash::new("test").unwrap(),
        )
        .unwrap()
    }

    fn test_logger() -> Logger {
        Logger::root(slog::Discard, o!())
    }

    fn test_fields(schema: &InputSchema) -> Vec<Field> {
        fn non_null_type(name: &str) -> Type {
            Type::NonNullType(Box::new(Type::NamedType(name.to_string())))
        }

        let schema = schema.schema();
        vec![
            Field::new(schema, "id", &non_null_type("ID"), false),
            Field::new(schema, "owner", &non_null_type("Bytes"), false),
            Field::new(schema, "displayName", &non_null_type("String"), false),
            Field::new(schema, "imageUrl", &non_null_type("String"), false),
        ]
    }

    #[test]
    fn test_get_fields_of() {
        let schema = test_schema();

        let entity_type = schema.entity_type("Gravatar").unwrap();
        let fields = &entity_type.object_type().unwrap().fields;

        assert_eq!(fields, &test_fields(&schema).into_boxed_slice());
    }

    #[test]
    fn test_query_string() {
        let base = test_base();
        let id = test_id();
        let schema = test_schema();
        let logger = test_logger();

        let fork = SubgraphFork::new(base, id, schema.clone(), logger).unwrap();

        let query = Query {
            query: fork
                .query_string("Gravatar", &test_fields(&schema))
                .unwrap(),
            variables: Variables {
                id: "0x00".to_string(),
            },
        };
        assert_eq!(
            query,
            Query {
                query: r#"query Query ($id: String) {
    gravatar(id: $id, subgraphError: allow) {
        id owner displayName imageUrl
    }
}"#
                .to_string(),
                variables: Variables {
                    id: "0x00".to_string()
                },
            }
        );
    }

    #[test]
    fn test_extract_entity() {
        let schema = test_schema();
        let entity = SubgraphFork::extract_entity(
            &schema,
            r#"{
    "data": {
        "gravatar": {
            "id": "0x00",
            "owner": "0x01",
            "displayName": "test",
            "imageUrl": "http://example.com/image.png"
        }
    }
}"#,
            "Gravatar",
            &test_fields(&schema),
        )
        .unwrap();

        assert_eq!(
            entity.unwrap(),
            schema
                .make_entity(vec![
                    ("id".into(), Value::String("0x00".to_string())),
                    (
                        "owner".into(),
                        Value::Bytes(scalar::Bytes::from_str("0x01").unwrap())
                    ),
                    ("displayName".into(), Value::String("test".to_string())),
                    (
                        "imageUrl".into(),
                        Value::String("http://example.com/image.png".to_string())
                    ),
                ])
                .unwrap()
        );
    }
}
