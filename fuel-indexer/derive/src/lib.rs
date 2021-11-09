extern crate proc_macro;
use proc_macro::TokenStream;

mod handler;
mod schema;
use handler::process_handler_attr;
use schema::process_graphql_schema;

#[proc_macro_error::proc_macro_error]
#[proc_macro]
pub fn graphql_schema(inputs: TokenStream) -> TokenStream {
    process_graphql_schema(inputs)
}

#[proc_macro_error::proc_macro_error]
#[proc_macro_attribute]
pub fn handler(attrs: TokenStream, item: TokenStream) -> TokenStream {
    process_handler_attr(attrs, item)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_macros() {
        let t = trybuild::TestCases::new();

        t.compile_fail("test_data/fail_self.rs");
        t.compile_fail("test_data/fail_args.rs");
        t.pass("test_data/success.rs");
        t.compile_fail("test_data/fail_noschema.rs");
        //t.compile_fail("test_data/fail_badschema.rs");
    }
}
