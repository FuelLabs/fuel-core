use crate::{
    db::Conn,
    db::models::{
        GraphRoot, NewColumn, NewGraphRoot, NewRootColumns, TypeIds,
    },
    graphql::Schema,
    sql_types::ColumnType,
    type_id,
};
use diesel::result::QueryResult;
use diesel::{sql_query, Connection, RunQueryDsl};
use graphql_parser::parse_schema;
use graphql_parser::schema::{Definition, Field, SchemaDefinition, Type, TypeDefinition};
use std::collections::{HashMap, HashSet};

cfg_if::cfg_if! {
    if #[cfg(feature = "diesel-postgres")] {
        fn create_schema(statements: &mut Vec<String>, namespace: &String) {
            let create = format!("CREATE SCHEMA IF NOT EXISTS {}", namespace);
            statements.push(create);
        }
        fn format_table(namespace: &String, table: &String) -> String {
            format!("{}.{}", namespace, table)
        }
    } else if #[cfg(feature = "diesel-sqlite")] {
        fn create_schema(_statements: &mut Vec<String>, _namespace: &String) {
        }
        fn format_table(_namespace: &String, table: &String) -> String {
            format!("{}", table)
        }
    }
}

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
        create_schema(&mut self.statements, &self.namespace);

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

    pub fn commit_metadata(self, conn: &Conn) -> QueryResult<Schema> {
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
                column_type: typ.to_string(),
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
            column_type: ColumnType::Blob.to_string(),
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
                    "CREATE TABLE IF NOT EXISTS\n {} (\n {}\n)",
                    format_table(&self.namespace, &table_name), columns,
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

    #[cfg(feature = "diesel-postgres")]
    #[test]
    fn test_schema_builder() {
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

        let sb = SchemaBuilder::new("test_namespace", "a_version_string");

        let SchemaBuilder { statements, .. } = sb.build(GRAPHQL_SCHEMA);

        assert_eq!(statements[0], CREATE_SCHEMA);
        assert_eq!(statements[1], CREATE_THING1);
        assert_eq!(statements[2], CREATE_THING2);
    }

    #[cfg(feature = "diesel-sqlite")]
    #[test]
    fn test_schema_builder() {
        const CREATE_THING1: &str = concat!(
            "CREATE TABLE IF NOT EXISTS\n",
            " thing1 (\n",
            " id bigint primary key not null,\n",
            "account varchar(64) not null,\n",
            "object bytea not null",
            "\n)"
        );
        const CREATE_THING2: &str = concat!(
            "CREATE TABLE IF NOT EXISTS\n",
            " thing2 (\n",
            " id bigint primary key not null,\n",
            "account varchar(64) not null,\n",
            "hash varchar(64) not null,\n",
            "object bytea not null\n",
            ")"
        );

        let sb = SchemaBuilder::new("test_namespace", "a_version_string");

        let SchemaBuilder { statements, .. } = sb.build(GRAPHQL_SCHEMA);

        assert_eq!(statements[0], CREATE_THING1);
        assert_eq!(statements[1], CREATE_THING2);
    }
}
