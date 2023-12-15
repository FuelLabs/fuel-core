use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{self, parse_macro_input, Field, ItemStruct};

pub fn generate_from_rust_type(metadata: TokenStream, input: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(input as ItemStruct);
    let args = parse_macro_input!(metadata as super::Args);

    let enum_names = args
        .vars
        .iter()
        .filter(|f| f.ident != super::REQUIRED_IDENT_NAME)
        .map(|f| f.ident.to_string())
        .collect::<Vec<String>>();

    let required_flds = args
        .vars
        .iter()
        .filter(|f| f.ident == super::REQUIRED_IDENT_NAME)
        .flat_map(|f| f.fields.named.iter())
        .map(|f| f.ident.as_ref().unwrap().to_string())
        .collect::<Vec<String>>();

    //struct's standard fields
    let fields = item_struct
        .fields
        .iter()
        .filter(|f| {
            let nm = f.ident.as_ref().unwrap().to_string();
            !enum_names.contains(&nm) && !nm.starts_with('_')
        })
        .collect::<Vec<&Field>>();

    //struct's enum fields
    let enum_fields = item_struct
        .fields
        .iter()
        .filter(|f| enum_names.contains(&f.ident.as_ref().unwrap().to_string()))
        .collect::<Vec<&Field>>();

    //module name
    let mod_name = Ident::new(
        &format!("__{}__", item_struct.ident.to_string().to_lowercase()),
        item_struct.ident.span(),
    );

    let name = item_struct.ident.clone();
    let asc_name = Ident::new(&format!("Asc{}", name), Span::call_site());

    //generate enum fields validator
    let enum_validation = enum_fields.iter().map(|f|{
        let fld_name = f.ident.as_ref().unwrap(); //empty, maybe call it "sum"?
        let type_nm = format!("\"{}\"", name).parse::<proc_macro2::TokenStream>().unwrap();
        let fld_nm = format!("\"{}\"", fld_name).parse::<proc_macro2::TokenStream>().unwrap();

        quote! {
            let #fld_name = self.#fld_name.as_ref()
                .ok_or_else(||  graph::runtime::HostExportError::from(graph::runtime::DeterministicHostError::from(anyhow::anyhow!("{} missing {}", #type_nm, #fld_nm))))?;
            }
    });

    let mut methods:Vec<proc_macro2::TokenStream> =
        fields.iter().map(|f| {
            let fld_name = f.ident.as_ref().unwrap();
            let self_ref =
                if is_byte_array(f){
                    quote! { graph_runtime_wasm::asc_abi::class::Bytes(&self.#fld_name) }
                }else{
                    quote!{ self.#fld_name }
                };

            let is_required = is_required(f, &required_flds);

            let setter =
                if is_nullable(f) {
                    if is_required{
                        let type_nm = format!("\"{}\"", name).parse::<proc_macro2::TokenStream>().unwrap();
                        let fld_nm = format!("\"{}\"", fld_name).parse::<proc_macro2::TokenStream>().unwrap();

                        quote! {
                            #fld_name: graph::runtime::asc_new_or_missing(heap, &#self_ref, gas, #type_nm, #fld_nm)?,
                        }
                    }else{
                        quote! {
                            #fld_name: graph::runtime::asc_new_or_null(heap, &#self_ref, gas)?,
                        }
                    }
                } else if is_scalar(&field_type(f)){
                    quote!{
                        #fld_name: #self_ref,
                    }
                }else{
                    quote! {
                        #fld_name: graph::runtime::asc_new(heap, &#self_ref, gas)?,
                    }
                };
            setter
        })
        .collect();

    for var in args.vars {
        let var_nm = var.ident.to_string();
        if var_nm == super::REQUIRED_IDENT_NAME {
            continue;
        }

        let mut c = var_nm.chars();
        let var_type_name = c.next().unwrap().to_uppercase().collect::<String>() + c.as_str();

        var.fields.named.iter().map(|f|{

            let fld_nm = f.ident.as_ref().unwrap();
            let var_nm = var.ident.clone();

            use heck::{ToUpperCamelCase, ToSnakeCase};

            let varian_type_name = fld_nm.to_string().to_upper_camel_case();
            let mod_name = item_struct.ident.to_string().to_snake_case();
            let varian_type_name = format!("{}::{}::{}",mod_name, var_type_name, varian_type_name).parse::<proc_macro2::TokenStream>().unwrap();

            if is_byte_array(f){
                    quote! {
                        #fld_nm: if let #varian_type_name(v) = #var_nm {graph::runtime::asc_new(heap, &graph_runtime_wasm::asc_abi::class::Bytes(v), gas)? } else {graph::runtime::AscPtr::null()},
                    }
                }else{
                    quote! {
                        #fld_nm: if let #varian_type_name(v) = #var_nm {graph::runtime::asc_new(heap, v, gas)? } else {graph::runtime::AscPtr::null()},
                    }
                }
        })
        .for_each(|ts| methods.push(ts));
    }

    let expanded = quote! {
        #item_struct

        #[automatically_derived]
        mod #mod_name{
            use super::*;

            use crate::protobuf::*;

            impl graph::runtime::ToAscObj<#asc_name> for #name {

                #[allow(unused_variables)]
                fn to_asc_obj<H: graph::runtime::AscHeap + ?Sized>(
                    &self,
                    heap: &mut H,
                    gas: &graph::runtime::gas::GasCounter,
                ) -> Result<#asc_name, graph::runtime::HostExportError> {

                    #(#enum_validation)*

                    Ok(
                         #asc_name {
                            #(#methods)*
                            ..Default::default()
                        }
                    )
                }
            }
        } // -------- end of mod


    };

    expanded.into()
}

fn is_scalar(fld: &str) -> bool {
    match fld {
        "i8" | "u8" => true,
        "i16" | "u16" => true,
        "i32" | "u32" => true,
        "i64" | "u64" => true,
        "usize" | "isize" => true,
        "bool" => true,
        _ => false,
    }
}

fn field_type(fld: &syn::Field) -> String {
    if let syn::Type::Path(tp) = &fld.ty {
        if let Some(ps) = tp.path.segments.last() {
            ps.ident.to_string()
        } else {
            "N/A".into()
        }
    } else {
        "N/A".into()
    }
}

fn is_required(fld: &syn::Field, req_list: &[String]) -> bool {
    let fld_name = fld.ident.as_ref().unwrap().to_string();
    req_list.iter().any(|r| r == &fld_name)
}

fn is_nullable(fld: &syn::Field) -> bool {
    if let syn::Type::Path(tp) = &fld.ty {
        if let Some(last) = tp.path.segments.last() {
            return last.ident == "Option";
        }
    }
    false
}

fn is_byte_array(fld: &syn::Field) -> bool {
    if let syn::Type::Path(tp) = &fld.ty {
        if let Some(last) = tp.path.segments.last() {
            if last.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(ref v) = last.arguments {
                    if let Some(last) = v.args.last() {
                        if let syn::GenericArgument::Type(t) = last {
                            if let syn::Type::Path(p) = t {
                                if let Some(a) = p.path.segments.last() {
                                    return a.ident == "u8";
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}
