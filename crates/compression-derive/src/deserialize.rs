use proc_macro2::TokenStream as TokenStream2;
use quote::{
    format_ident,
    quote,
};

use crate::attribute::{
    should_skip_field,
    should_skip_field_binding,
    StructAttrs,
};

fn deserialize_struct(s: &mut synstructure::Structure) -> TokenStream2 {
    assert_eq!(s.variants().len(), 1, "structs must have one variant");

    let variant: &synstructure::VariantInfo = &s.variants()[0];
    let decode_main = variant.construct(|field, _| {
        let ty = &field.ty;
        if should_skip_field(&field.attrs) {
            quote! {
                ::core::default::Default::default()
            }
        } else {
            quote! {{
                <#ty as ::fuel_types::canonical::Deserialize>::decode_static(buffer)?
            }}
        }
    });

    let decode_dynamic = variant.each(|binding| {
        if should_skip_field_binding(binding) {
            quote! {
                *#binding = ::core::default::Default::default();
            }
        } else {
            quote! {{
                ::fuel_types::canonical::Deserialize::decode_dynamic(#binding, buffer)?;
            }}
        }
    });

    let remove_prefix = if let Some(expected_prefix) = StructAttrs::parse(s).prefix {
        quote! {{
            let prefix = <_ as ::fuel_types::canonical::Deserialize>::decode_static(buffer);
            if prefix != Ok(#expected_prefix) {
                return ::core::result::Result::Err(::fuel_types::canonical::Error::InvalidPrefix)
            }
        }}
    } else {
        quote! {}
    };

    s.gen_impl(quote! {
        gen impl ::fuel_types::canonical::Deserialize for @Self {
            fn decode_static<I: ::fuel_types::canonical::Input + ?Sized>(buffer: &mut I) -> ::core::result::Result<Self, ::fuel_types::canonical::Error> {
                #remove_prefix
                ::core::result::Result::Ok(#decode_main)
            }

            fn decode_dynamic<I: ::fuel_types::canonical::Input + ?Sized>(&mut self, buffer: &mut I) -> ::core::result::Result<(), ::fuel_types::canonical::Error> {
                match self {
                    #decode_dynamic,
                };
                ::core::result::Result::Ok(())
            }
        }
    })
}

fn deserialize_enum(s: &synstructure::Structure) -> TokenStream2 {
    let _name = &s.ast().ident;

    assert!(!s.variants().is_empty(), "got invalid empty enum");

    let mut next_discriminant = quote! { { 0u64 } };
    let enum_ident = &s.ast().ident;
    let calculated_discriminants =
        s.variants().iter().enumerate().map(|(index, variant)| {
            if variant.ast().discriminant.is_some() {
                let variant_ident = variant.ast().ident;
                next_discriminant = quote! { { #enum_ident::#variant_ident as u64 } };
            }

            let const_ident = format_ident!("V{}", index);
            let result = quote! { const #const_ident: ::core::primitive::u64 =  #next_discriminant; };

            next_discriminant = quote! { ( (#next_discriminant) + 1u64 ) };

            result
        });

    let decode_static: TokenStream2 = s
        .variants()
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let decode_main = variant.construct(|field, _| {
                if should_skip_field(&field.attrs) {
                    quote! {
                        ::core::default::Default::default()
                    }
                } else {
                    let ty = &field.ty;
                    quote! {{
                        <#ty as ::fuel_types::canonical::Deserialize>::decode_static(buffer)?
                    }}
                }
            });
            let const_ident = format_ident!("V{}", index);

            quote! {
                #const_ident => {
                    ::core::result::Result::Ok(#decode_main)
                }
            }
        })
        .collect();

    let decode_dynamic = s.variants().iter().map(|variant| {
        let decode_dynamic = variant.each(|binding| {
            if !should_skip_field_binding(binding) {
                quote! {{
                    ::fuel_types::canonical::Deserialize::decode_dynamic(#binding, buffer)?;
                }}
            } else {
                quote! {}
            }
        });

        quote! {
            #decode_dynamic
        }
    });

    let discriminant = {
        quote! {
            <::core::primitive::u64 as ::fuel_types::canonical::Deserialize>::decode(buffer)?
        }
    };

    s.gen_impl(quote! {
        gen impl ::fuel_types::canonical::Deserialize for @Self {
            fn decode_static<I: ::fuel_types::canonical::Input + ?Sized>(buffer: &mut I) -> ::core::result::Result<Self, ::fuel_types::canonical::Error> {
                #( #calculated_discriminants )*

                match #discriminant {
                    #decode_static
                    _ => ::core::result::Result::Err(::fuel_types::canonical::Error::UnknownDiscriminant),
                }
            }

            fn decode_dynamic<I: ::fuel_types::canonical::Input + ?Sized>(&mut self, buffer: &mut I) -> ::core::result::Result<(), ::fuel_types::canonical::Error> {
                match self {
                    #(
                        #decode_dynamic
                    )*
                    _ => return ::core::result::Result::Err(::fuel_types::canonical::Error::UnknownDiscriminant),
                };

                ::core::result::Result::Ok(())
            }
        }
    })
}

/// Derives `Deserialize` trait for the given `struct` or `enum`.
pub fn deserialize_derive(mut s: synstructure::Structure) -> TokenStream2 {
    s.bind_with(|_| synstructure::BindStyle::RefMut)
        .add_bounds(synstructure::AddBounds::Fields)
        .underscore_const(true);

    match s.ast().data {
        syn::Data::Struct(_) => deserialize_struct(&mut s),
        syn::Data::Enum(_) => deserialize_enum(&s),
        _ => panic!("Can't derive `Deserialize` for `union`s"),
    }
}
