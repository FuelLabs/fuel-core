table! {
    use diesel::sql_types::*;
    use crate::sql_types::*;

    columns (id) {
        id -> Integer,
        type_id -> BigInt,
        column_position -> Integer,
        column_name -> Text,
        column_type -> Text,
        nullable -> Bool,
        graphql_type -> Text,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::sql_types::*;

    graph_root (id) {
        id -> Integer,
        version -> Text,
        schema_name -> Text,
        query -> Text,
        schema -> Text,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::sql_types::*;

    root_columns (id) {
        id -> Integer,
        root_id -> Integer,
        column_name -> Text,
        graphql_type -> Text,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::sql_types::*;

    type_ids (id) {
        id -> BigInt,
        schema_version -> Text,
        schema_name -> Text,
        graphql_name -> Text,
        table_name -> Text,
    }
}

joinable!(columns -> type_ids (type_id));
joinable!(root_columns -> graph_root (root_id));

allow_tables_to_appear_in_same_query!(
    columns,
    graph_root,
    root_columns,
    type_ids,
);
