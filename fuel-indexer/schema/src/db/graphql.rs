use graphql_parser::query as gql;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

type GraphqlResult<T> = Result<T, GraphqlError>;

#[derive(Debug, Error)]
pub enum GraphqlError {
    #[error("GraphQl Parser error: {0:?}")]
    ParseError(#[from] gql::ParseError),
    #[error("Unrecognized Type: {0:?}")]
    UnrecognizedType(String),
    #[error("Unrecognized Field in {0:?}: {1:?}")]
    UnrecognizedField(String, String),
    #[error("Unrecognized Argument in {0:?}: {1:?}")]
    UnrecognizedArgument(String, String),
    #[error("Operation not supported: {0:?}")]
    OperationNotSupported(String),
    #[error("Fragment for {0:?} can't be used within {1:?}.")]
    InvalidFragmentSelection(Fragment, String),
    #[error("Unsupported Value Type: {0:?}")]
    UnsupportedValueType(String),
    #[error("Failed to resolve query fragments.")]
    FragmentResolverFailed,
    #[error("Selection not supported.")]
    SelectionNotSupported,
}

pub struct Schema {
    namespace: String,
    query: String,
    types: HashSet<String>,
    fields: HashMap<String, HashMap<String, String>>,
}

impl Schema {
    pub fn new(
        namespace: String,
        query: String,
        types: HashSet<String>,
        fields: HashMap<String, HashMap<String, String>>,
    ) -> Schema {
        Schema {
            namespace,
            query,
            types,
            fields,
        }
    }

    pub fn check_type(&self, type_name: &str) -> bool {
        self.types.contains(type_name)
    }

    pub fn field_type(&self, cond: &str, name: &str) -> Option<&String> {
        match self.fields.get(cond) {
            Some(fieldset) => fieldset.get(name),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Selection {
    Field(String, Vec<Filter>, Selections),
    Fragment(String),
}

#[derive(Clone, Debug)]
pub struct Filter {
    name: String,
    value: String,
}

impl Filter {
    pub fn new(name: String, value: String) -> Filter {
        Filter { name, value }
    }

    pub fn as_sql(&self) -> String {
        format!("{} = {}", self.name, self.value)
    }
}

#[derive(Clone, Debug)]
pub struct Selections {
    _field_type: String,
    has_fragments: bool,
    selections: Vec<Selection>,
}

impl Selections {
    pub fn new<'a>(
        schema: &Schema,
        field_type: &str,
        set: &gql::SelectionSet<'a, &'a str>,
    ) -> GraphqlResult<Selections> {
        let mut selections = Vec::with_capacity(set.items.len());
        let mut has_fragments = false;

        for item in &set.items {
            match item {
                gql::Selection::Field(field) => {
                    // TODO: directives and sub-selections for nested types...
                    let gql::Field {
                        name,
                        selection_set,
                        arguments,
                        ..
                    } = field;

                    let subfield_type = schema.field_type(field_type, name).ok_or_else(|| {
                        GraphqlError::UnrecognizedField(field_type.into(), name.to_string())
                    })?;

                    let mut filters = vec![];
                    for (arg, value) in arguments {
                        if schema.field_type(subfield_type, arg).is_none() {
                            return Err(GraphqlError::UnrecognizedArgument(
                                subfield_type.into(),
                                arg.to_string(),
                            ));
                        }

                        let val = match value {
                            gql::Value::Int(val) => format!("{}", val.as_i64().unwrap()),
                            gql::Value::Float(val) => format!("{}", val),
                            gql::Value::String(val) => val.to_string(),
                            gql::Value::Boolean(val) => format!("{}", val),
                            gql::Value::Null => String::from("NULL"),
                            o => {
                                return Err(GraphqlError::UnsupportedValueType(format!("{:#?}", o)))
                            }
                        };

                        filters.push(Filter::new(arg.to_string(), val));
                    }

                    let sub_selections = Selections::new(schema, subfield_type, selection_set)?;
                    selections.push(Selection::Field(name.to_string(), filters, sub_selections));
                }
                gql::Selection::FragmentSpread(frag) => {
                    let gql::FragmentSpread { fragment_name, .. } = frag;
                    has_fragments = true;
                    selections.push(Selection::Fragment(fragment_name.to_string()));
                }
                // Inline fragments not handled yet....
                _ => return Err(GraphqlError::SelectionNotSupported),
            }
        }

        Ok(Selections {
            _field_type: field_type.to_string(),
            has_fragments,
            selections,
        })
    }

    pub fn resolve_fragments(
        &mut self,
        schema: &Schema,
        cond: &str,
        fragments: &HashMap<String, Fragment>,
    ) -> GraphqlResult<usize> {
        let mut has_fragments = false;
        let mut resolved = 0;
        let mut selections = Vec::new();

        for selection in &mut self.selections {
            match selection {
                Selection::Fragment(name) => {
                    if let Some(frag) = fragments.get(name) {
                        if !frag.check_cond(cond) {
                            return Err(GraphqlError::InvalidFragmentSelection(
                                frag.clone(),
                                cond.to_string(),
                            ));
                        }
                        resolved += 1;
                        selections.extend(frag.selections.get_selections());
                    } else {
                        has_fragments = true;
                        selections.push(Selection::Fragment(name.to_string()));
                    }
                }
                Selection::Field(name, filters, sub_selection) => {
                    let field_type = schema.field_type(cond, name).unwrap();
                    let _ = sub_selection.resolve_fragments(schema, field_type, fragments)?;

                    selections.push(Selection::Field(
                        name.to_string(),
                        filters.to_vec(),
                        sub_selection.clone(),
                    ));
                }
            }
        }

        self.selections = selections;
        self.has_fragments = has_fragments;
        Ok(resolved)
    }

    pub fn get_selections(&self) -> Vec<Selection> {
        self.selections.clone()
    }
}

#[derive(Clone, Debug)]
pub struct Fragment {
    cond: String,
    selections: Selections,
}

impl Fragment {
    pub fn new<'a>(
        schema: &Schema,
        cond: String,
        selection_set: &gql::SelectionSet<'a, &'a str>,
    ) -> GraphqlResult<Fragment> {
        let selections = Selections::new(schema, &cond, selection_set)?;

        Ok(Fragment { cond, selections })
    }

    pub fn check_cond(&self, cond: &str) -> bool {
        self.cond == cond
    }

    pub fn has_fragments(&self) -> bool {
        self.selections.has_fragments
    }

    /// Return the number of fragments resolved
    pub fn resolve_fragments(
        &mut self,
        schema: &Schema,
        fragments: &HashMap<String, Fragment>,
    ) -> GraphqlResult<usize> {
        self.selections
            .resolve_fragments(schema, &self.cond, fragments)
    }
}

#[derive(Debug)]
pub struct Operation {
    namespace: String,
    _name: String,
    selections: Selections,
}

impl Operation {
    pub fn new(namespace: String, name: String, selections: Selections) -> Operation {
        Operation {
            namespace,
            _name: name,
            selections,
        }
    }

    pub fn as_sql(&self) -> Vec<String> {
        let Operation {
            namespace,
            selections,
            ..
        } = self;
        let mut queries = Vec::new();

        // TODO: nested queries, joins, etc....
        for selection in selections.get_selections() {
            if let Selection::Field(name, filters, selections) = selection {
                let columns: Vec<_> = selections
                    .get_selections()
                    .into_iter()
                    .filter_map(|f| {
                        if let Selection::Field(name, ..) = f {
                            Some(name)
                        } else {
                            None
                        }
                    })
                    .collect();

                let column_text = columns.join(", ");

                let mut query = format!(
                    "SELECT {} FROM {}.{}",
                    column_text,
                    namespace,
                    name.to_lowercase()
                );

                if !filters.is_empty() {
                    let filter_text: String = filters.iter().map(Filter::as_sql).join(" AND ");
                    query.push_str(format!(" WHERE {}", filter_text).as_str());
                }

                queries.push(query)
            }
        }

        queries
    }
}

#[derive(Debug)]
pub struct GraphqlQuery {
    operations: Vec<Operation>,
}

impl GraphqlQuery {
    pub fn as_sql(&self) -> Vec<String> {
        let mut queries = Vec::new();

        for op in &self.operations {
            queries.extend(op.as_sql());
        }

        queries
    }
}

pub struct GraphqlQueryBuilder<'a> {
    schema: &'a Schema,
    document: gql::Document<'a, &'a str>,
}

impl<'a> GraphqlQueryBuilder<'a> {
    pub fn new(schema: &'a Schema, query: &'a str) -> GraphqlResult<GraphqlQueryBuilder<'a>> {
        let document = gql::parse_query::<&str>(query)?;
        Ok(GraphqlQueryBuilder { schema, document })
    }

    pub fn build(self) -> GraphqlResult<GraphqlQuery> {
        let fragments = self.process_fragments()?;
        let operations = self.process_operations(fragments)?;

        Ok(GraphqlQuery { operations })
    }

    fn process_operation(
        &self,
        operation: &gql::OperationDefinition<'a, &'a str>,
        fragments: &HashMap<String, Fragment>,
    ) -> GraphqlResult<Operation> {
        match operation {
            gql::OperationDefinition::SelectionSet(set) => {
                let selections = Selections::new(self.schema, &self.schema.query, set)?;

                Ok(Operation::new(
                    self.schema.namespace.clone(),
                    "Unnamed".into(),
                    selections,
                ))
            }
            gql::OperationDefinition::Query(q) => {
                // TODO: directives and variable definitions....
                let gql::Query {
                    name,
                    selection_set,
                    ..
                } = q;
                let name = name.map_or_else(|| "Unnamed".into(), |o| o.into());

                let mut selections =
                    Selections::new(self.schema, &self.schema.query, selection_set)?;
                selections.resolve_fragments(self.schema, &self.schema.query, fragments)?;

                Ok(Operation::new(
                    self.schema.namespace.clone(),
                    name,
                    selections,
                ))
            }
            gql::OperationDefinition::Mutation(_) => {
                Err(GraphqlError::OperationNotSupported("Mutation".into()))
            }
            gql::OperationDefinition::Subscription(_) => {
                Err(GraphqlError::OperationNotSupported("Subscription".into()))
            }
        }
    }

    fn process_operations(
        &self,
        fragments: HashMap<String, Fragment>,
    ) -> GraphqlResult<Vec<Operation>> {
        let mut operations = vec![];

        for def in &self.document.definitions {
            if let gql::Definition::Operation(operation) = def {
                let op = self.process_operation(operation, &fragments)?;

                operations.push(op);
            }
        }

        Ok(operations)
    }

    fn process_fragments(&self) -> GraphqlResult<HashMap<String, Fragment>> {
        let mut fragments = HashMap::new();
        let mut to_resolve = Vec::new();

        for def in &self.document.definitions {
            if let gql::Definition::Fragment(frag) = def {
                let gql::FragmentDefinition {
                    name,
                    type_condition,
                    selection_set,
                    ..
                } = frag;

                let gql::TypeCondition::On(cond) = type_condition;

                if !self.schema.check_type(cond) {
                    return Err(GraphqlError::UnrecognizedType(cond.to_string()));
                }

                let frag = Fragment::new(self.schema, cond.to_string(), selection_set)?;

                if frag.has_fragments() {
                    to_resolve.push((name.to_string(), frag));
                } else {
                    fragments.insert(name.to_string(), frag);
                }
            }
        }

        loop {
            let mut resolved = 0;
            let mut remaining = Vec::new();

            for (name, mut frag) in to_resolve.into_iter() {
                resolved += frag.resolve_fragments(self.schema, &fragments)?;

                if frag.has_fragments() {
                    remaining.push((name, frag))
                } else {
                    fragments.insert(name, frag);
                }
            }

            if !remaining.is_empty() && resolved == 0 {
                return Err(GraphqlError::FragmentResolverFailed);
            } else if remaining.is_empty() {
                break;
            }

            to_resolve = remaining;
        }

        Ok(fragments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::FromIterator;

    fn generate_schema() -> Schema {
        let t = ["Address", "Bytes32", "ID", "Thing1", "Thing2"]
            .iter()
            .map(|s| s.to_string());
        let types = HashSet::from_iter(t);

        let f1 = HashMap::from_iter(
            [("thing1", "Thing1"), ("thing2", "Thing2")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        let f2 = HashMap::from_iter(
            [("id", "ID"), ("account", "Address")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        let f3 = HashMap::from_iter(
            [("id", "ID"), ("account", "Address"), ("hash", "Bytes32")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        let fields = HashMap::from_iter([
            ("Query".to_string(), f1),
            ("Thing1".to_string(), f2),
            ("Thing2".to_string(), f3),
        ]);
        Schema::new("test_namespace".to_string(), "Query".into(), types, fields)
    }

    #[test]
    fn test_graphql_queries() {
        let good_query = r#"
            fragment otherfrag on Thing2 {
                id
            }

            fragment frag1 on Thing2 {
                account
                hash
                ...otherfrag
            }

            query GetThing2 {
                thing2(id: 1234) {
                    account
                    hash
                }
            }

            query OtherQuery {
                thing2(id: 84848) {
                    ...frag1
                }
            }

            {
                thing1(id: 4321) { account }
            }
        "#;

        let schema = generate_schema();

        let query = GraphqlQueryBuilder::new(&schema, good_query);
        assert!(query.is_ok());
        let q = query.expect("It's ok here").build();
        assert!(q.is_ok());
        let q = q.expect("It's ok");
        let sql = q.as_sql();

        assert_eq!(
            vec![
                "SELECT account, hash FROM test_namespace.thing2 WHERE id = 1234".to_string(),
                "SELECT account, hash, id FROM test_namespace.thing2 WHERE id = 84848".to_string(),
                "SELECT account FROM test_namespace.thing1 WHERE id = 4321".to_string(),
            ],
            sql
        );

        let bad_query = r#"
            fragment frag1 on BadType{
                account
            }

            query GetThing2 {
                thing2(id: 123) {
                    ...frag
                }
            }
        "#;

        let query = GraphqlQueryBuilder::new(&schema, bad_query);
        assert!(query.is_ok());
        match query.expect("It's ok here").build() {
            Err(GraphqlError::UnrecognizedType(_)) => (),
            o => panic!("Should have gotten Unrecognized type, got {:?}", o),
        }
    }
}
