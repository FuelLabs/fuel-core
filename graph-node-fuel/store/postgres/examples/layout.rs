extern crate clap;
extern crate graph_store_postgres;

use clap::{arg, Command};
use graph::schema::InputSchema;
use std::collections::BTreeSet;
use std::process::exit;
use std::{fs, sync::Arc};

use graph::prelude::DeploymentHash;
use graph_store_postgres::{
    command_support::{Catalog, Column, ColumnType, Layout, Namespace},
    layout_for_tests::make_dummy_site,
};

pub fn usage(msg: &str) -> ! {
    println!("layout: {}", msg);
    println!("Try 'layout --help' for more information.");
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

fn print_drop(layout: &Layout) {
    for table in layout.tables.values() {
        println!("drop table {};", table.qualified_name);
    }
}

fn print_delete_all(layout: &Layout) {
    for table in layout.tables.values() {
        println!("delete from {};", table.qualified_name);
    }
}

fn print_ddl(layout: &Layout) {
    let ddl = ensure(layout.as_ddl(), "Failed to generate DDL");
    println!("{}", ddl);
}

fn print_diesel_tables(layout: &Layout) {
    fn diesel_type(column: &Column) -> String {
        let mut dsl_type = match column.column_type {
            ColumnType::Boolean => "Bool",
            ColumnType::BigDecimal | ColumnType::BigInt => "Numeric",
            ColumnType::Bytes => "Binary",
            ColumnType::Int => "Integer",
            ColumnType::Int8 => "Int8",
            ColumnType::String | ColumnType::Enum(_) | ColumnType::TSVector(_) => "Text",
        }
        .to_owned();

        if column.is_list() {
            dsl_type = format!("Array<{}>", dsl_type);
        }
        if column.is_nullable() {
            dsl_type = format!("Nullable<{}>", dsl_type);
        }
        dsl_type
    }

    fn rust_type(column: &Column) -> String {
        let mut dsl_type = match column.column_type {
            ColumnType::Boolean => "bool",
            ColumnType::BigDecimal | ColumnType::BigInt => "BigDecimal",
            ColumnType::Bytes => "Vec<u8>",
            ColumnType::Int => "i32",
            ColumnType::Int8 => "i64",
            ColumnType::String | ColumnType::Enum(_) | ColumnType::TSVector(_) => "String",
        }
        .to_owned();

        if column.is_list() {
            dsl_type = format!("Vec<{}>", dsl_type);
        }
        if column.is_nullable() {
            dsl_type = format!("Option<{}>", dsl_type);
        }
        dsl_type
    }

    let mut tables = layout.tables.values().collect::<Vec<_>>();
    tables.sort_by_key(|table| table.name.as_str());

    for table in &tables {
        println!("    table! {{");
        let name = table.qualified_name.as_str().replace('\"', "");
        println!("        {} (vid) {{", name);
        println!("            vid -> BigInt,");
        for column in &table.as_ref().columns {
            println!(
                "            {} -> {},",
                column.name.as_str(),
                diesel_type(column)
            );
        }
        println!("            block_range -> Range<Integer>,");
        println!("        }}");
        println!("    }}\n")
    }

    // Now generate Rust structs for all this
    for table in &tables {
        println!("    #[derive(Queryable, Clone, Debug)]");
        println!("    pub struct {} {{", table.object);
        println!("        pub vid: i64,");
        for column in &table.as_ref().columns {
            println!(
                "        pub {}: {},",
                column.name.as_str(),
                rust_type(column)
            );
        }
        println!("        pub block_range: (Bound<i32>, Bound<i32>),");
        println!("    }}\n")
    }
}

pub fn main() {
    let args = Command::new("layout")
    .version("1.0")
    .about("Information about the database schema for a GraphQL schema")
    .arg(arg!(-g --generate <KIND> "what kind of SQL to generate. Can be ddl (the default), migrate, delete, or drop"))
    .arg(arg!(<schema>))
    .arg(arg!(<db_schema>))
    .get_matches();

    let schema = args.get_one::<&str>("schema").unwrap();
    let namespace = args.get_one::<&str>("db_schema").unwrap_or(&"subgraphs");
    let kind = args.get_one::<&str>("generate").unwrap_or(&"ddl");

    let subgraph = DeploymentHash::new("Qmasubgraph").unwrap();
    let schema = ensure(fs::read_to_string(schema), "Can not read schema file");
    let schema = ensure(
        InputSchema::parse(&schema, subgraph.clone()),
        "Failed to parse schema",
    );
    let namespace = ensure(
        Namespace::new(namespace.to_string()),
        "Invalid database schema",
    );
    let site = Arc::new(make_dummy_site(subgraph, namespace, "anet".to_string()));
    let catalog = ensure(
        Catalog::for_tests(site.clone(), BTreeSet::new()),
        "Failed to construct catalog",
    );
    let layout = ensure(
        Layout::new(site, &schema, catalog),
        "Failed to construct Mapping",
    );
    match *kind {
        "drop" => print_drop(&layout),
        "delete" => print_delete_all(&layout),
        "ddl" => print_ddl(&layout),
        "diesel" => print_diesel_tables(&layout),
        _ => {
            usage(&format!("illegal value {} for --generate", kind));
        }
    }
}
