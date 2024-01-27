use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::attribute::{
    should_skip_field_binding,
    StructAttrs,
};

fn serialize_struct(s: &synstructure::Structure) -> TokenStream2 {
    assert_eq!(s.variants().len(), 1, "structs must have one variant");
    let variant: &synstructure::VariantInfo = &s.variants()[0];

    let name = s.ast().ident;
    let compact_name = format_ident!("Compact{}", name);

    let compact_fields = variant.each(|binding| {
        
    });

    s.gen_impl(quote! {
        pub struct #compact_name {
            #compact_fields
        }

        gen impl ::fuel_core_compression::Compactable for @Self {
            type Compact = #compact_name;

            fn count(&self) -> CountPerTable {
                #count_per_field;
            }

            fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
            where
                R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex {
                #compact_per_field;
            }

            fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
            where
                R: db::RegistryRead {
                #decompact_per_field;
            }
        }
    })
}

fn serialize_enum(s: &synstructure::Structure) -> TokenStream2 {
    assert!(!s.variants().is_empty(), "got invalid empty enum");
    let mut next_discriminant = quote! { { 0u64 } };
    let encode_static = s.variants().iter().map(|v| {
        let pat = v.pat();
        let encode_static_iter = v.bindings().iter().map(|binding| {
            if should_skip_field_binding(binding) {
                quote! {}
            } else {
                quote! {
                    ::fuel_types::canonical::Serialize::encode_static(#binding, buffer)?;
                }
            }
        });

        if v.ast().discriminant.is_some() {
            let variant_ident = v.ast().ident;
            next_discriminant = quote! { { Self::#variant_ident as u64 } };
        }

        let encode_discriminant = quote! {
            <::core::primitive::u64 as ::fuel_types::canonical::Serialize>::encode(&#next_discriminant, buffer)?;
        };
        next_discriminant = quote! { ( (#next_discriminant) + 1u64 ) };

        quote! {
            #pat => {
                #encode_discriminant
                #(
                    { #encode_static_iter }
                )*
            }
        }
    });
    let encode_dynamic = s.variants().iter().map(|v| {
        let encode_dynamic_iter = v.each(|binding| {
            if should_skip_field_binding(binding) {
                quote! {}
            } else {
                quote! {
                    ::fuel_types::canonical::Serialize::encode_dynamic(#binding, buffer)?;
                }
            }
        });
        quote! {
            #encode_dynamic_iter
        }
    });

    let match_size_static: TokenStream2 = s
        .variants()
        .iter()
        .map(|variant| {
            variant.each(|binding| {
                if should_skip_field_binding(binding) {
                    quote! {}
                } else {
                    quote! {
                        size = ::fuel_types::canonical::add_sizes(size, #binding.size_static());
                    }
                }
            })
        })
        .collect();
    let match_size_static = quote! {{
        // `repr(128)` is unstable, so because of that we can use 8 bytes.
        let mut size = 8;
        match self { #match_size_static } size }
    };

    let match_size_dynamic: TokenStream2 = s
        .variants()
        .iter()
        .map(|variant| {
            variant.each(|binding| {
                if should_skip_field_binding(binding) {
                    quote! {}
                } else {
                    quote! {
                        size = ::fuel_types::canonical::add_sizes(size, #binding.size_dynamic());
                    }
                }
            })
        })
        .collect();
    let match_size_dynamic =
        quote! {{ let mut size = 0; match self { #match_size_dynamic } size }};

    let impl_code = s.gen_impl(quote! {
        gen impl ::fuel_types::canonical::Serialize for @Self {
            #[inline(always)]
            fn size_static(&self) -> usize {
                #match_size_static
            }

            #[inline(always)]
            fn size_dynamic(&self) -> usize {
                #match_size_dynamic
            }

            #[inline(always)]
            fn encode_static<O: ::fuel_types::canonical::Output + ?Sized>(&self, buffer: &mut O) -> ::core::result::Result<(), ::fuel_types::canonical::Error> {
                match self {
                    #(
                        #encode_static
                    )*,
                    _ => return ::core::result::Result::Err(::fuel_types::canonical::Error::UnknownDiscriminant),
                };

                ::core::result::Result::Ok(())
            }

            fn encode_dynamic<O: ::fuel_types::canonical::Output + ?Sized>(&self, buffer: &mut O) -> ::core::result::Result<(), ::fuel_types::canonical::Error> {
                match self {
                    #(
                        #encode_dynamic
                    )*,
                    _ => return ::core::result::Result::Err(::fuel_types::canonical::Error::UnknownDiscriminant),
                };

                ::core::result::Result::Ok(())
            }
        }
    });

    quote! {
        #impl_code
    }
}

/// Derives `Serialize` trait for the given `struct` or `enum`.
pub fn serialize_derive(mut s: synstructure::Structure) -> TokenStream2 {
    s.add_bounds(synstructure::AddBounds::Fields)
        .underscore_const(true);

    match s.ast().data {
        syn::Data::Struct(_) => serialize_struct(&s),
        syn::Data::Enum(_) => serialize_enum(&s),
        _ => panic!("Can't derive `Serialize` for `union`s"),
    }
}
