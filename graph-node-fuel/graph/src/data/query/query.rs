use serde::de::Deserializer;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::{
    data::graphql::shape_hash::shape_hash,
    prelude::{q, r, ApiVersion, DeploymentHash, SubgraphName, ENV_VARS},
};

fn deserialize_number<'de, D>(deserializer: D) -> Result<q::Number, D::Error>
where
    D: Deserializer<'de>,
{
    let i: i32 = Deserialize::deserialize(deserializer)?;
    Ok(q::Number::from(i))
}

fn deserialize_list<'de, D>(deserializer: D) -> Result<Vec<q::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    let values: Vec<DeserializableGraphQlValue> = Deserialize::deserialize(deserializer)?;
    Ok(values.into_iter().map(|v| v.0).collect())
}

fn deserialize_object<'de, D>(deserializer: D) -> Result<BTreeMap<String, q::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    let pairs: BTreeMap<String, DeserializableGraphQlValue> =
        Deserialize::deserialize(deserializer)?;
    Ok(pairs.into_iter().map(|(k, v)| (k, v.0)).collect())
}

#[derive(Deserialize)]
#[serde(untagged, remote = "q::Value")]
enum GraphQLValue {
    #[serde(deserialize_with = "deserialize_number")]
    Int(q::Number),
    Float(f64),
    String(String),
    Boolean(bool),
    Null,
    Enum(String),
    #[serde(deserialize_with = "deserialize_list")]
    List(Vec<q::Value>),
    #[serde(deserialize_with = "deserialize_object")]
    Object(BTreeMap<String, q::Value>),
}

/// Variable value for a GraphQL query.
#[derive(Clone, Debug, Deserialize)]
pub struct DeserializableGraphQlValue(#[serde(with = "GraphQLValue")] q::Value);

fn deserialize_variables<'de, D>(deserializer: D) -> Result<HashMap<String, r::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let pairs: BTreeMap<String, DeserializableGraphQlValue> =
        Deserialize::deserialize(deserializer)?;
    pairs
        .into_iter()
        .map(|(k, DeserializableGraphQlValue(v))| r::Value::try_from(v).map(|v| (k, v)))
        .collect::<Result<_, _>>()
        .map_err(|v| D::Error::custom(format!("failed to convert to r::Value: {:?}", v)))
}

/// Variable values for a GraphQL query.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct QueryVariables(
    #[serde(deserialize_with = "deserialize_variables")] HashMap<String, r::Value>,
);

impl QueryVariables {
    pub fn new(variables: HashMap<String, r::Value>) -> Self {
        QueryVariables(variables)
    }
}

impl Deref for QueryVariables {
    type Target = HashMap<String, r::Value>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for QueryVariables {
    fn deref_mut(&mut self) -> &mut HashMap<String, r::Value> {
        &mut self.0
    }
}

impl serde::ser::Serialize for QueryVariables {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, &v)?;
        }
        map.end()
    }
}

#[derive(Clone, Debug)]
pub enum QueryTarget {
    Name(SubgraphName, ApiVersion),
    Deployment(DeploymentHash, ApiVersion),
}

impl QueryTarget {
    pub fn get_version(&self) -> &ApiVersion {
        match self {
            Self::Deployment(_, version) | Self::Name(_, version) => version,
        }
    }
}

/// A GraphQL query as submitted by a client, either directly or through a subscription.
#[derive(Clone, Debug)]
pub struct Query {
    pub document: q::Document,
    pub variables: Option<QueryVariables>,
    pub shape_hash: u64,
    pub query_text: Arc<String>,
    pub variables_text: Arc<String>,
    pub trace: bool,
    _force_use_of_new: (),
}

impl Query {
    pub fn new(document: q::Document, variables: Option<QueryVariables>, trace: bool) -> Self {
        let shape_hash = shape_hash(&document);

        let (query_text, variables_text) = if trace
            || ENV_VARS.log_gql_timing()
            || (ENV_VARS.graphql.enable_validations && ENV_VARS.graphql.silent_graphql_validations)
        {
            (
                document
                    .format(graphql_parser::Style::default().indent(0))
                    .replace('\n', " "),
                serde_json::to_string(&variables).unwrap_or_default(),
            )
        } else {
            ("(gql logging turned off)".to_owned(), "".to_owned())
        };

        Query {
            document,
            variables,
            shape_hash,
            query_text: Arc::new(query_text),
            variables_text: Arc::new(variables_text),
            trace,
            _force_use_of_new: (),
        }
    }
}
