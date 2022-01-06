use fuel_indexer_schema::{schema_version, type_id, BASE_SCHEMA};
use graphql_parser::parse_schema;
use graphql_parser::schema::{Definition, Document, Field, SchemaDefinition, Type, TypeDefinition};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, LitStr, Result, Token};

/// Arguments to this proc macro are (<namespace>, <gaphql_file>)
struct GraphSchema {
    namespace: LitStr,
    path: LitStr,
}

impl Parse for GraphSchema {
    fn parse(input: ParseStream) -> Result<GraphSchema> {
        let namespace = input.parse()?;
        let _: Token![,] = input.parse()?;
        let path = input.parse()?;

        Ok(GraphSchema { namespace, path })
    }
}

fn process_type<'a>(
    types: &HashSet<String>,
    typ: &Type<'a, String>,
    nullable: bool,
) -> proc_macro2::TokenStream {
    match typ {
        Type::NamedType(t) => {
            if !types.contains(t) {
                panic!("Type {} is undefined.", t);
            }

            let id = format_ident! {"{}", t };

            if nullable {
                quote! { Option<#id> }
            } else {
                quote! { #id }
            }
        }
        Type::ListType(_t) => panic!("Got a list type, we don't handle this yet..."),
        Type::NonNullType(t) => process_type(types, t, false),
    }
}

fn process_field<'a>(
    types: &HashSet<String>,
    field: &Field<'a, String>,
) -> (
    proc_macro2::TokenStream,
    proc_macro2::Ident,
    proc_macro2::TokenStream,
) {
    // TODO: might want to make use of directives on fields?
    //       e.g. to annotate columns to be indexed in postgres.

    let Field {
        name, field_type, ..
    } = field;
    let typ = process_type(types, field_type, true);
    let ident = format_ident! {"{}", name};

    let extractor = quote! {
        let item = vec.pop().expect("Missing item in row");
        let #ident = match item {
            FtColumn::#typ(t) => t,
            _ => panic!("Invalid column type {:?}", item),
        };

    };

    (typ, ident, extractor)
}

fn process_type_def<'a>(
    query_root: &str,
    namespace: &str,
    types: &HashSet<String>,
    typ: &TypeDefinition<'a, String>,
) -> Option<proc_macro2::TokenStream> {
    match typ {
        TypeDefinition::Object(obj) => {
            if obj.name == *query_root {
                return None;
            }
            let name = &obj.name;
            let type_id = type_id(namespace, name);
            // TODO: ignore directives for now, could do some useful things with them though.
            let mut block = quote! {};
            let mut row_extractors = quote! {};
            let mut construction = quote! {};
            let mut flattened = quote! {};

            for field in &obj.fields {
                let (type_name, field_name, ext) = process_field(types, field);

                block = quote! {
                    #block
                    #field_name: #type_name,
                };

                row_extractors = quote! {
                    #ext

                    #row_extractors
                };

                construction = quote! {
                    #construction
                    #field_name,
                };

                flattened = quote! {
                    #flattened
                    FtColumn::#type_name(self.#field_name),
                };
            }
            let strct = format_ident! {"{}", name};

            Some(quote! {
                #[derive(Debug, PartialEq, Eq)]
                pub struct #strct {
                    #block
                }

                impl Entity for #strct {
                    const TYPE_ID: u64 = #type_id;

                    fn from_row(mut vec: Vec<FtColumn>) -> Self {
                        #row_extractors
                        Self {
                            #construction
                        }
                    }

                    fn to_row(&self) -> Vec<FtColumn> {
                        vec![
                            #flattened
                        ]
                    }
                }
            })
        }
        obj => panic!("Unexpected type: {:?}", obj),
    }
}

fn process_definition<'a>(
    query_root: &str,
    namespace: &str,
    types: &HashSet<String>,
    definition: &Definition<'a, String>,
) -> Option<proc_macro2::TokenStream> {
    match definition {
        Definition::TypeDefinition(def) => process_type_def(query_root, namespace, types, def),
        Definition::SchemaDefinition(_def) => None,
        def => {
            panic!("Unhandled definition type: {:?}", def);
        }
    }
}

fn type_name(typ: &TypeDefinition<String>) -> String {
    match typ {
        TypeDefinition::Scalar(obj) => obj.name.clone(),
        TypeDefinition::Object(obj) => obj.name.clone(),
        TypeDefinition::Interface(obj) => obj.name.clone(),
        TypeDefinition::Union(obj) => obj.name.clone(),
        TypeDefinition::Enum(obj) => obj.name.clone(),
        TypeDefinition::InputObject(obj) => obj.name.clone(),
    }
}

fn get_query_root<'a>(types: &HashSet<String>, ast: &Document<'a, String>) -> String {
    let schema = ast.definitions.iter().find_map(|def| {
        if let Definition::SchemaDefinition(d) = def {
            Some(d)
        } else {
            None
        }
    });

    if schema.is_none() {
        panic!("Schema definition not found!");
    }

    let SchemaDefinition { query, .. } = schema.unwrap();

    if query.is_none() {
        panic!("Schema definition must specify a query root!");
    }

    let name = query.as_ref().unwrap().into();

    if !types.contains(&name) {
        panic!("Query root not defined!");
    }

    name
}

fn get_schema_types(ast: &Document<String>) -> (HashSet<String>, HashSet<String>) {
    let types: HashSet<String> = ast
        .definitions
        .iter()
        .filter_map(|def| {
            if let Definition::TypeDefinition(typ) = def {
                Some(typ)
            } else {
                None
            }
        })
        .map(type_name)
        .collect();

    let directives = ast
        .definitions
        .iter()
        .filter_map(|def| {
            if let Definition::DirectiveDefinition(dir) = def {
                Some(dir.name.clone())
            } else {
                None
            }
        })
        .collect();

    (types, directives)
}

fn const_item(id: &str, value: &str) -> proc_macro2::TokenStream {
    let ident = format_ident! {"{}", id};

    let fn_ptr = format_ident! {"get_{}_ptr", id.to_lowercase()};
    let fn_len = format_ident! {"get_{}_len", id.to_lowercase()};

    quote! {
        const #ident: &'static str = #value;

        #[no_mangle]
        fn #fn_ptr() -> *const u8 {
            #ident.as_ptr()
        }

        #[no_mangle]
        fn #fn_len() -> u32 {
            #ident.len() as u32
        }
    }
}

pub(crate) fn process_graphql_schema(inputs: TokenStream) -> TokenStream {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("Manifest dir unknown");

    let mut current = std::path::PathBuf::from(manifest);
    let schema = parse_macro_input!(inputs as GraphSchema);
    current.push(schema.path.value());

    let mut file = match File::open(current) {
        Ok(f) => f,
        Err(e) => {
            proc_macro_error::abort_call_site!("Could not open schema file {:?}", e)
        }
    };

    let mut text = String::new();
    file.read_to_string(&mut text).expect("IO error");

    let base_ast = match parse_schema::<String>(BASE_SCHEMA) {
        Ok(ast) => ast,
        Err(e) => {
            proc_macro_error::abort_call_site!("Error parsing graphql schema {:?}", e)
        }
    };
    let (primitives, _) = get_schema_types(&base_ast);

    let ast = match parse_schema::<String>(&text) {
        Ok(ast) => ast,
        Err(e) => {
            proc_macro_error::abort_call_site!("Error parsing graphql schema {:?}", e)
        }
    };
    let (mut types, _) = get_schema_types(&ast);
    types.extend(primitives);

    let namespace = const_item("NAMESPACE", &schema.namespace.value());
    let version = const_item("VERSION", &schema_version(&text));

    let mut output = quote! {
        use alloc::{vec, vec::Vec};
        use fuel_indexer::Entity;
        use fuel_indexer::types::*;
        #namespace
        #version
    };

    let query_root = get_query_root(&types, &ast);

    for definition in ast.definitions.iter() {
        if let Some(def) =
            process_definition(&query_root, &schema.namespace.value(), &types, definition)
        {
            output = quote! {
                #output
                #def
            };
        }
    }

    TokenStream::from(output)
}
