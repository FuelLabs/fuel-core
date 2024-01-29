use proc_macro2::TokenStream as TokenStream2;
use quote::{
    format_ident,
    quote,
};

use crate::attribute::{
    FieldAttrs,
    StructureAttrs,
};

pub struct PerField {
    defs: TokenStream2,
    count: TokenStream2,
}
impl PerField {
    fn form(fields: &syn::Fields) -> Self {
        let mut defs = TokenStream2::new();
        let mut count = TokenStream2::new();

        for field in fields {
            let attrs = FieldAttrs::parse(&field.attrs);
            defs.extend(match &attrs {
            FieldAttrs::Skip => quote! {},
            FieldAttrs::Normal => {
                let ty = &field.ty;
                let cty = quote! {
                    <#ty as ::fuel_core_compression::Compactable>::Compact
                };
                if let Some(fname) = field.ident.as_ref() {
                    quote! { #fname: #cty, }
                } else {
                    quote! { #cty, }
                }
            }
            FieldAttrs::Registry(registry) => {
                let reg_ident = format_ident!("{}", registry);
                let cty = quote! {
                    ::fuel_core_compression::Key<::fuel_core_compression::tables::#reg_ident>
                };
                if let Some(fname) = field.ident.as_ref() {
                    quote! { #fname: #cty, }
                } else {
                    quote! { #cty, }
                }
            }
        });
            count.extend(match &attrs {
                FieldAttrs::Skip => quote! { CountPerTable::default() + },
                FieldAttrs::Normal => {
                    let ty = &field.ty;
                    quote! {
                        <#ty as ::fuel_core_compression::Compactable>::Compact::count() +
                    }
                }
                FieldAttrs::Registry(registry) => {
                    quote! {
                        CountPerTable { #registry: 1, ..CountPerTable::default() } +
                    }
                }
            });
        }

        let defs = match fields {
            syn::Fields::Named(_) => quote! {{ #defs }},
            syn::Fields::Unnamed(_) => quote! {(#defs)},
            syn::Fields::Unit => quote! {},
        };
        count.extend(quote! { 0 });

        Self { defs, count }
    }
}

fn serialize_struct(s: &synstructure::Structure) -> TokenStream2 {
    assert_eq!(s.variants().len(), 1, "structs must have one variant");
    let variant: &synstructure::VariantInfo = &s.variants()[0];

    let name = &s.ast().ident;
    let compact_name = format_ident!("Compact{}", name);

    let PerField {
        defs,
        count: count_per_field,
    } = PerField::form(&variant.ast().fields);

    let g = s.ast().generics.clone();
    let w = g.where_clause.clone();
    let compact = quote! {
        #[derive(Debug, Clone)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct #compact_name #g #w #defs ;
    };

    let impls = s.gen_impl(quote! {
        use ::fuel_core_compression::{db, CountPerTable, CompactionContext};

        gen impl ::fuel_core_compression::Compactable for @Self {

            type Compact = #compact_name #g;

            fn count(&self) -> CountPerTable {
                // #count_per_field;
                todo!()
            }

            fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
            where
                R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex {
                // #compact_per_field;
                todo!()
            }

            fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
            where
                R: db::RegistryRead {
                // #decompact_per_field;
                todo!()
            }
        }
    });

    quote! {
        #compact
        #impls
    }
}

fn serialize_enum(s: &synstructure::Structure) -> TokenStream2 {
    assert!(!s.variants().is_empty(), "got invalid empty enum");

    let name = &s.ast().ident;
    let compact_name = format_ident!("Compact{}", name);

    let enumdef = quote! {
        #[derive(Debug, Clone)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum #compact_name
    };

    let mut variantdefs = TokenStream2::new();
    let mut counts = Vec::new();

    for variant in s.variants() {
        let vname = variant.ast().ident.clone();

        let PerField { defs, count } = PerField::form(&variant.ast().fields);

        variantdefs.extend(quote! {
            #vname #defs,
        });
        counts.push(count);
    }

    let impls = s.gen_impl(quote! {
        use ::fuel_core_compression::{db, CountPerTable, CompactionContext};

        gen impl ::fuel_core_compression::Compactable for @Self {

            type Compact = #compact_name;

            fn count(&self) -> CountPerTable {
                // #count_per_field;
                todo!()
            }

            fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
            where
                R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex {
                // #compact_per_field;
                todo!()
            }

            fn decompact<R>(compact: Self::Compact, reg: &R) -> Self
            where
                R: db::RegistryRead {
                // #decompact_per_field;
                todo!()
            }
        }
    });
    quote! {
        #enumdef { #variantdefs }
        #impls
    }
}

fn serialize_transparent(s: synstructure::Structure) -> TokenStream2 {
    assert_eq!(
        s.variants().len(),
        1,
        "transparent structures must have one variant"
    );
    let variant: &synstructure::VariantInfo = &s.variants()[0];
    assert_eq!(
        variant.ast().fields.len(),
        1,
        "transparent structures must have exactly one field"
    );
    let field_t = variant.ast().fields.iter().next().unwrap().ty.clone();
    let field_d =
        quote! { <#field_t as Compactable>::decompact(c, reg) };
    let field_name: TokenStream2 = match variant.ast().fields {
        syn::Fields::Named(n) => {
            let n = n.named[0].ident.clone().unwrap();
            quote! { #n }
        }
        syn::Fields::Unnamed(_) => quote! { 0 },
        syn::Fields::Unit => unreachable!(),
    };
    let field_c = match variant.ast().fields {
        syn::Fields::Named(_) => quote! { Self {#field_name: #field_d} },
        syn::Fields::Unnamed(_) => quote! { Self(#field_d) },
        syn::Fields::Unit => unreachable!(),
    };

    s.gen_impl(quote! {
        use ::fuel_core_compression::{db, Compactable, CountPerTable, CompactionContext};

        gen impl Compactable for @Self {
            type Compact = #field_t;

            fn count(&self) -> CountPerTable {
                self.#field_name.count()
            }

            fn compact<R>(&self, ctx: &mut CompactionContext<R>) -> Self::Compact
            where
                R: db::RegistryRead + db::RegistryWrite + db::RegistryIndex {
                self.#field_name.compact(ctx)
            }

            fn decompact<R>(c: Self::Compact, reg: &R) -> Self
            where
                R: db::RegistryRead {
                #field_c
            }
        }
    })
}

/// Derives `Serialize` trait for the given `struct` or `enum`.
pub fn serialize_derive(mut s: synstructure::Structure) -> TokenStream2 {
    s.add_bounds(synstructure::AddBounds::Fields)
        .underscore_const(true);

    let ts = match StructureAttrs::parse(&s.ast().attrs) {
        StructureAttrs::Normal => match s.ast().data {
            syn::Data::Struct(_) => serialize_struct(&s),
            syn::Data::Enum(_) => serialize_enum(&s),
            _ => panic!("Can't derive `Serialize` for `union`s"),
        },
        StructureAttrs::Transparent => serialize_transparent(s),
    };
    println!("{}", ts);
    ts
}
