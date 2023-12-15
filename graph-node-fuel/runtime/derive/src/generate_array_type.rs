use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{self, parse_macro_input, AttributeArgs, ItemStruct, Meta, NestedMeta, Path};

pub fn generate_array_type(metadata: TokenStream, input: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(input as ItemStruct);
    let name = item_struct.ident.clone();

    let asc_name = Ident::new(&format!("Asc{}", name), Span::call_site());
    let asc_name_array = Ident::new(&format!("Asc{}Array", name), Span::call_site());

    let args = parse_macro_input!(metadata as AttributeArgs);

    let args = args
        .iter()
        .filter_map(|a| {
            if let NestedMeta::Meta(Meta::Path(Path { segments, .. })) = a {
                if let Some(p) = segments.last() {
                    return Some(p.ident.to_string());
                }
            }
            None
        })
        .collect::<Vec<String>>();

    assert!(
        !args.is_empty(),
        "arguments not found! generate_array_type(<network-name>)"
    );

    let no_asc_name = if name.to_string().to_uppercase().starts_with("ASC") {
        name.to_string()[3..].to_owned()
    } else {
        name.to_string()
    };

    let index_asc_type_id_array = format!("{}{}Array", args[0], no_asc_name)
        .parse::<proc_macro2::TokenStream>()
        .unwrap();

    quote! {
        #item_struct

        #[automatically_derived]
        pub struct #asc_name_array(pub  graph_runtime_wasm::asc_abi::class::Array<graph::runtime::AscPtr<#asc_name>>);

        impl graph::runtime::ToAscObj<#asc_name_array> for Vec<#name> {
            fn to_asc_obj<H: graph::runtime::AscHeap + ?Sized>(
                &self,
                heap: &mut H,
                gas: &graph::runtime::gas::GasCounter,
            ) -> Result<#asc_name_array, graph::runtime::HostExportError> {
                let content: Result<Vec<_>, _> = self.iter().map(|x| graph::runtime::asc_new(heap, x, gas)).collect();

                Ok(#asc_name_array(graph_runtime_wasm::asc_abi::class::Array::new(&content?, heap, gas)?))
            }
        }

        impl graph::runtime::AscType for #asc_name_array {
            fn to_asc_bytes(&self) -> Result<Vec<u8>, graph::runtime::DeterministicHostError> {
                self.0.to_asc_bytes()
            }

            fn from_asc_bytes(
                asc_obj: &[u8],
                api_version: &graph::semver::Version,
            ) -> Result<Self, graph::runtime::DeterministicHostError> {
                Ok(Self(graph_runtime_wasm::asc_abi::class::Array::from_asc_bytes(asc_obj, api_version)?))
            }
        }

        #[automatically_derived]
        impl graph::runtime::AscIndexId for #asc_name_array {
            const INDEX_ASC_TYPE_ID: graph::runtime::IndexForAscTypeId = graph::runtime::IndexForAscTypeId::#index_asc_type_id_array ;
        }

    }
    .into()
}
