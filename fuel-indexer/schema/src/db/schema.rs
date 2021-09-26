pub mod graph_registry {
    table! {
        use diesel::sql_types::*;
        use crate::sql_types::*;

        graph_registry.columns (id) {
            id -> Int4,
            type_id -> Int8,
            column_position -> Int4,
            column_name -> Varchar,
            column_type -> Columntypename,
            nullable -> Bool,
        }
    }

    table! {
        use diesel::sql_types::*;
        use crate::sql_types::*;

        graph_registry.type_ids (id) {
            id -> Int8,
            schema_version -> Varchar,
            schema_name -> Varchar,
            graphql_name -> Varchar,
            table_name -> Varchar,
        }
    }

    joinable!(columns -> type_ids (type_id));

    allow_tables_to_appear_in_same_query!(columns, type_ids,);
}
