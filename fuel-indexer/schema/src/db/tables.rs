use crate::{
    db::models::{NewColumn, TypeIds},
    sql_types::ColumnType,
    type_id,
};
use diesel::prelude::PgConnection;
use diesel::result::QueryResult;
use graphql_parser::parse_schema;
use graphql_parser::schema::{Definition, Field, SchemaDefinition, Type, TypeDefinition};

#[derive(Default)]
pub struct SchemaBuilder {
    statements: Vec<String>,
    type_ids: Vec<TypeIds>,
    columns: Vec<NewColumn>,
    namespace: String,
    version: String,
}

impl SchemaBuilder {
    pub fn new(namespace: &str, version: &str) -> SchemaBuilder {
        SchemaBuilder {
            namespace: namespace.to_string(),
            version: version.to_string(),
            ..Default::default()
        }
    }

    pub fn build(mut self, schema: &str) -> Schema {
        let create = format!("CREATE SCHEMA IF NOT EXISTS {}", self.namespace);
        self.statements.push(create);

        let ast = match parse_schema::<String>(schema) {
            Ok(ast) => ast,
            Err(e) => panic!("Error parsing graphql schema {:?}", e),
        };

        let root = ast
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

        if root.is_none() {
            panic!("TODO: this needs to be error type");
        }

        let root = root.cloned().unwrap();

        for def in ast.definitions.iter() {
            if let Definition::TypeDefinition(typ) = def {
                self.generate_table_sql(&root, typ);
            }
        }

        let SchemaBuilder {
            statements,
            type_ids,
            columns,
            ..
        } = self;

        Schema {
            root,
            statements,
            type_ids,
            columns,
        }
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
            nullable: false,
        };

        fragments.push(object_column.sql_fragment());
        self.columns.push(object_column);

        fragments.join(",\n")
    }

    fn generate_table_sql<'a>(&mut self, root: &str, typ: &TypeDefinition<'a, String>) {
        match typ {
            TypeDefinition::Object(o) => {
                if o.name == root {
                    return;
                }

                let type_id = type_id(&o.name);
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

pub struct Schema {
    pub root: String,
    pub statements: Vec<String>,
    pub type_ids: Vec<TypeIds>,
    pub columns: Vec<NewColumn>,
}

impl Schema {
    pub fn commit_metadata(&self, conn: &PgConnection) -> QueryResult<()> {
        let Schema {
            type_ids, columns, ..
        } = self;

        for type_id in type_ids {
            type_id.insert(conn)?;
        }

        for column in columns {
            column.insert(conn)?;
        }

        Ok(())
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
        " id bigint not null,\n",
        "account varchar(64) not null,\n",
        "object bytea not null",
        "\n)"
    );
    const CREATE_THING2: &str = concat!(
        "CREATE TABLE IF NOT EXISTS\n",
        " test_namespace.thing2 (\n",
        " id bigint not null,\n",
        "account varchar(64) not null,\n",
        "hash varchar(64) not null,\n",
        "object bytea not null\n",
        ")"
    );

    #[test]
    fn test_schema_builder() {
        let sb = SchemaBuilder::new("test_namespace", "a_version_string");

        let Schema {
            root, statements, ..
        } = sb.build(GRAPHQL_SCHEMA);

        assert_eq!(root, "QueryRoot");
        assert_eq!(statements[0], CREATE_SCHEMA);
        assert_eq!(statements[1], CREATE_THING1);
        assert_eq!(statements[2], CREATE_THING2);
    }
}
