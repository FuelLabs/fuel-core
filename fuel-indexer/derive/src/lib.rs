extern crate proc_macro;
use proc_macro::TokenStream;

mod schema;
mod handler;
use schema::process_graphql_schema;
use handler::process_handler_attr;


#[proc_macro]
pub fn graphql_schema(inputs: TokenStream) -> TokenStream {
    process_graphql_schema(inputs)
}


#[proc_macro_attribute]
pub fn handler(attrs: TokenStream, item: TokenStream) -> TokenStream {
    process_handler_attr(attrs, item)
}
