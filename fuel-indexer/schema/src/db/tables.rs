use crate::{
    db::models::{
        Columns, GraphRoot, NewColumn, NewGraphRoot, NewRootColumns, RootColumns, TypeIds,
    },
    sql_types::ColumnType,
    type_id,
};
use diesel::result::QueryResult;
use diesel::{prelude::PgConnection, sql_query, Connection, RunQueryDsl};
use graphql_parser::parse_schema;
use graphql_parser::schema::{Definition, Field, SchemaDefinition, Type, TypeDefinition};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct SchemaBuilder {
    statements: Vec<String>,
    type_ids: Vec<TypeIds>,
    columns: Vec<NewColumn>,
    namespace: String,
    version: String,
    schema: String,
    types: HashSet<String>,
    fields: HashMap<String, HashMap<String, String>>,
    query: String,
    query_fields: HashMap<String, HashMap<String, String>>,
}

impl SchemaBuilder {
    pub fn new(namespace: &str, version: &str) -> SchemaBuilder {
        SchemaBuilder {
            namespace: namespace.to_string(),
            version: version.to_string(),
            ..Default::default()
        }
    }

    pub fn build(mut self, schema: &str) -> Self {
        let create = format!("CREATE SCHEMA IF NOT EXISTS {}", self.namespace);
        self.statements.push(create);

        let ast = match parse_schema::<String>(schema) {
            Ok(ast) => ast,
            Err(e) => panic!("Error parsing graphql schema {:?}", e),
        };

        let query = ast
            .definitions
            .iter()
            .filter_map(|s| {
                if let Definition::SchemaDefinition(def) = s {
                    let SchemaDefinition { query, .. } = def;
                    query.as_ref()
                } else {
                    None
                }
            })
            .next();

        if query.is_none() {
            panic!("TODO: this needs to be error type");
        }

        let query = query.cloned().unwrap();

        for def in ast.definitions.iter() {
            if let Definition::TypeDefinition(typ) = def {
                self.generate_table_sql(&query, typ);
            }
        }

        self.query = query;
        self.schema = schema.to_string();

        self
    }

    pub fn commit_metadata(self, conn: &PgConnection) -> QueryResult<Schema> {
        let SchemaBuilder {
            version,
            statements,
            type_ids,
            columns,
            namespace,
            types,
            fields,
            query,
            query_fields,
            schema,
        } = self;

        conn.transaction::<(), diesel::result::Error, _>(|| {
            NewGraphRoot {
                version: version.clone(),
                schema_name: namespace.clone(),
                query: query.clone(),
                schema,
            }
            .insert(conn)?;

            let latest = GraphRoot::get_latest(conn, &namespace)?;

            let fields = query_fields.get(&query).expect("No query root!");

            for (key, val) in fields {
                NewRootColumns {
                    root_id: latest.id,
                    column_name: key.to_string(),
                    graphql_type: val.to_string(),
                }
                .insert(conn)?;
            }

            for query in statements.iter() {
                sql_query(query).execute(conn)?;
            }

            for type_id in type_ids {
                type_id.insert(conn)?;
            }

            for column in columns {
                column.insert(conn)?;
            }
            Ok(())
        })?;

        Ok(Schema {
            version,
            namespace,
            query,
            types,
            fields,
        })
    }

    fn process_type<'a>(&self, field_type: &Type<'a, String>) -> (ColumnType, bool) {
        match field_type {
            Type::NamedType(t) => (ColumnType::from(t.as_str()), true),
            Type::ListType(_) => panic!("List types not supported yet."),
            Type::NonNullType(t) => {
                let (typ, _) = self.process_type(t);
                (typ, false)
            }
        }
    }

    fn generate_columns<'a>(&mut self, type_id: i64, fields: &[Field<'a, String>]) -> String {
        let mut fragments = vec![];

        for (pos, f) in fields.iter().enumerate() {
            // will ignore field arguments and field directives for now, but possibly useful...
            let (typ, nullable) = self.process_type(&f.field_type);

            let column = NewColumn {
                type_id,
                column_position: pos as i32,
                column_name: f.name.to_string(),
                column_type: typ,
                graphql_type: f.field_type.to_string(),
                nullable,
            };

            fragments.push(column.sql_fragment());
            self.columns.push(column);
        }

        let object_column = NewColumn {
            type_id,
            column_position: fragments.len() as i32,
            column_name: "object".to_string(),
            column_type: ColumnType::Blob,
            graphql_type: "__".into(),
            nullable: false,
        };

        fragments.push(object_column.sql_fragment());
        self.columns.push(object_column);

        fragments.join(",\n")
    }

    fn generate_table_sql<'a>(&mut self, root: &str, typ: &TypeDefinition<'a, String>) {
        fn map_fields(fields: &[Field<'_, String>]) -> HashMap<String, String> {
            fields
                .iter()
                .map(|f| (f.name.to_string(), f.field_type.to_string()))
                .collect()
        }
        match typ {
            TypeDefinition::Object(o) => {
                self.types.insert(o.name.to_string());
                self.fields
                    .insert(o.name.to_string(), map_fields(&o.fields));

                if o.name == root {
                    self.query_fields
                        .insert(root.to_string(), map_fields(&o.fields));
                    return;
                }

                let type_id = type_id(&self.namespace, &o.name);
                let columns = self.generate_columns(type_id as i64, &o.fields);
                let table_name = o.name.to_lowercase();

                let create = format!(
                    "CREATE TABLE IF NOT EXISTS\n {}.{} (\n {}\n)",
                    self.namespace, table_name, columns,
                );

                self.statements.push(create);
                self.type_ids.push(TypeIds {
                    id: type_id as i64,
                    schema_version: self.version.to_string(),
                    schema_name: self.namespace.to_string(),
                    graphql_name: o.name.to_string(),
                    table_name,
                });
            }
            o => panic!("Got a non-object type! {:?}", o),
        }
    }
}

#[derive(Debug)]
pub struct Schema {
    pub version: String,
    /// Graph ID, and the DB schema this data lives in.
    pub namespace: String,
    /// Root Graphql type.
    pub query: String,
    /// List of GraphQL type names.
    pub types: HashSet<String>,
    /// Mapping of key/value pairs per GraphQL type.
    pub fields: HashMap<String, HashMap<String, String>>,
}

impl Schema {
    pub fn load_from_db(conn: &PgConnection, name: &str) -> QueryResult<Self> {
        let root = GraphRoot::get_latest(conn, name)?;
        let root_cols = RootColumns::list_by_id(conn, root.id)?;
        let typeids = TypeIds::list_by_name(conn, &root.schema_name, &root.version)?;

        let mut types = HashSet::new();
        let mut fields = HashMap::new();

        types.insert(root.query.clone());
        fields.insert(
            root.query.clone(),
            root_cols
                .into_iter()
                .map(|c| (c.column_name, c.graphql_type))
                .collect(),
        );
        for tid in typeids {
            types.insert(tid.graphql_name.clone());

            let columns = Columns::list_by_id(conn, tid.id)?;
            fields.insert(
                tid.graphql_name,
                columns
                    .into_iter()
                    .map(|c| (c.column_name, c.graphql_type))
                    .collect(),
            );
        }

        Ok(Schema {
            version: root.version,
            namespace: root.schema_name,
            query: root.query,
            types,
            fields,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    const GRAPHQL_SCHEMA: &str = r#"
        schema {
            query: QueryRoot
        }
        
        type QueryRoot {
            thing1: Thing1
            thing2: Thing2
        }
        
        type Thing1 {
            id: ID!
            account: Address!
        }
        
        
        type Thing2 {
            id: ID!
            account: Address! @indexed
            hash: Bytes32! @indexed
        }
    "#;

    const CREATE_SCHEMA: &str = "CREATE SCHEMA IF NOT EXISTS test_namespace";
    const CREATE_THING1: &str = concat!(
        "CREATE TABLE IF NOT EXISTS\n",
        " test_namespace.thing1 (\n",
        " id bigint primary key not null,\n",
        "account varchar(64) not null,\n",
        "object bytea not null",
        "\n)"
    );
    const CREATE_THING2: &str = concat!(
        "CREATE TABLE IF NOT EXISTS\n",
        " test_namespace.thing2 (\n",
        " id bigint primary key not null,\n",
        "account varchar(64) not null,\n",
        "hash varchar(64) not null,\n",
        "object bytea not null\n",
        ")"
    );

    #[test]
    fn test_schema_builder() {
        let sb = SchemaBuilder::new("test_namespace", "a_version_string");

        let SchemaBuilder { statements, .. } = sb.build(GRAPHQL_SCHEMA);

        assert_eq!(statements[0], CREATE_SCHEMA);
        assert_eq!(statements[1], CREATE_THING1);
        assert_eq!(statements[2], CREATE_THING2);
    }
}
