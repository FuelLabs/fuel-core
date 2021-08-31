use syn::{
    bracketed,
    parse_macro_input,
    parse_quote,
    Attribute,
    Block,
    FnArg,
    LitStr,
    PatType,
    ItemFn,
    Token,
    token,
    Result
};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use proc_macro::TokenStream;
use quote::quote;


mod kw {
    syn::custom_keyword!(filters);
}


struct FiltersList {
    filters: Punctuated<Filter, Token![,]>,
}


impl Parse for FiltersList {
    fn parse(input: ParseStream) -> Result<FiltersList> {
        let content;
        let _: kw::filters = input.parse()?;
        let _: Token![=] = input.parse()?;
        let _: token::Bracket = bracketed!(content in input);

        Ok(FiltersList {
            filters: content.parse_terminated(Filter::parse)?,
        })
    }
}


struct Filter {
    name: LitStr,
}


impl Parse for Filter {
    fn parse(input: ParseStream) -> Result<Filter> {
        Ok(Filter {
            name: input.parse()?,
        })
    }
}


pub fn process_handler_attr(attrs: TokenStream, item: TokenStream) -> TokenStream {
    let FiltersList { filters } = parse_macro_input!(attrs as FiltersList);
    for filter in filters.into_iter() {
        println!("TJDEBUG filtered! {}", filter.name.value());
    }
    
    let mut item_fn = parse_macro_input!(item as ItemFn);

    let has_nomangle = item_fn.attrs
        .iter()
        .find(|attr| {
            let path = attr.path.get_ident();
            path.is_some() && path.unwrap().to_string() == String::from("no_mangle")
        })
        .is_some();

    if !has_nomangle {
        let no_mangle: Attribute = parse_quote! { #[no_mangle] };
        item_fn.attrs.push(no_mangle);
    };

    let mut sig: Punctuated<FnArg, Token![,]> = parse_quote! {
        ptr: *mut u8, len: usize
    };
    std::mem::swap(&mut sig, &mut item_fn.sig.inputs);

    let mut block: Block = parse_quote! {
        {
            use fuel_indexer::types::{serialize, deserialize};
            let vec = unsafe { Vec::from_raw_parts(ptr, len, len) };
        }
    };


    for item in sig.into_iter() {
        match item {
            FnArg::Typed(PatType { pat, ty, .. }) => {
                // TODO: for now, just assume that all events will be packed up
                //       into one object.... that should work.
                let stmt = parse_quote! {
                    let #pat: #ty = deserialize(&vec);
                };

                block.stmts.push(stmt);
            }
            FnArg::Receiver(_) => {
                panic!("'self' in function signature not allowed here")
            }
        }
    }

    std::mem::swap(&mut block, &mut item_fn.block);
    item_fn.block.stmts.extend(block.stmts);

    let output = quote!{
        #item_fn
    };

    proc_macro::TokenStream::from(output)
}
