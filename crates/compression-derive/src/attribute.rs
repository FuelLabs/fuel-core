use regex::Regex;

const ATTR: &str = "da_compress";

/// struct/enum attributes
pub enum StructureAttrs {
    /// Compacted recursively.
    Normal,
    /// Transparent.
    Transparent,
}
impl StructureAttrs {
    pub fn parse(attrs: &[syn::Attribute]) -> Self {
        let mut result = Self::Normal;
        for attr in attrs {
            if attr.style != syn::AttrStyle::Outer {
                continue;
            }

            if let syn::Meta::List(ml) = &attr.meta {
                if ml.path.segments.len() == 1 && ml.path.segments[0].ident == ATTR {
                    if !matches!(result, Self::Normal) {
                        panic!("Duplicate attribute: {}", ml.tokens);
                    }

                    let attr_contents = ml.tokens.to_string();
                    if attr_contents == "transparent" {
                        result = Self::Transparent;
                    } else {
                        panic!("Invalid attribute: {}", ml.tokens);
                    }
                }
            }
        }

        result
    }
}

/// Field attributes
pub enum FieldAttrs {
    /// Skipped when compacting, and must be reconstructed when decompacting.
    Skip,
    /// Compacted recursively.
    Normal,
    /// This value is compacted into a registry lookup.
    Registry(String),
}
impl FieldAttrs {
    pub fn parse(attrs: &[syn::Attribute]) -> Self {
        let re_registry = Regex::new(r#"^registry\s*=\s*"([a-zA-Z_]+)"$"#).unwrap();

        let mut result = Self::Normal;
        for attr in attrs {
            if attr.style != syn::AttrStyle::Outer {
                continue;
            }

            if let syn::Meta::List(ml) = &attr.meta {
                if ml.path.segments.len() == 1 && ml.path.segments[0].ident == ATTR {
                    if !matches!(result, Self::Normal) {
                        panic!("Duplicate attribute: {}", ml.tokens);
                    }

                    let attr_contents = ml.tokens.to_string();
                    if attr_contents == "skip" {
                        result = Self::Skip;
                    } else if let Some(m) = re_registry.captures(&attr_contents) {
                        result = Self::Registry(m.get(1).unwrap().as_str().to_owned());
                    } else {
                        panic!("Invalid attribute: {}", ml.tokens);
                    }
                }
            }
        }

        result
    }
}
