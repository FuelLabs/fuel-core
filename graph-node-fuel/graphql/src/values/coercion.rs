use graph::prelude::s::{EnumType, InputValue, ScalarType, Type, TypeDefinition};
use graph::prelude::{q, r, QueryExecutionError};
use graph::schema;
use std::collections::BTreeMap;
use std::convert::TryFrom;

/// A GraphQL value that can be coerced according to a type.
pub trait MaybeCoercible<T> {
    /// On error,  `self` is returned as `Err(self)`.
    fn coerce(self, using_type: &T) -> Result<r::Value, q::Value>;
}

impl MaybeCoercible<EnumType> for q::Value {
    fn coerce(self, using_type: &EnumType) -> Result<r::Value, q::Value> {
        match self {
            q::Value::Null => Ok(r::Value::Null),
            q::Value::String(name) | q::Value::Enum(name)
                if using_type.values.iter().any(|value| value.name == name) =>
            {
                Ok(r::Value::Enum(name))
            }
            _ => Err(self),
        }
    }
}

impl MaybeCoercible<ScalarType> for q::Value {
    fn coerce(self, using_type: &ScalarType) -> Result<r::Value, q::Value> {
        match (using_type.name.as_str(), self) {
            (_, q::Value::Null) => Ok(r::Value::Null),
            ("Boolean", q::Value::Boolean(b)) => Ok(r::Value::Boolean(b)),
            ("BigDecimal", q::Value::Float(f)) => Ok(r::Value::String(f.to_string())),
            ("BigDecimal", q::Value::Int(i)) => Ok(r::Value::String(
                i.as_i64().ok_or(q::Value::Int(i))?.to_string(),
            )),
            ("BigDecimal", q::Value::String(s)) => Ok(r::Value::String(s)),
            ("Int", q::Value::Int(num)) => {
                let n = num.as_i64().ok_or_else(|| q::Value::Int(num.clone()))?;
                if i32::min_value() as i64 <= n && n <= i32::max_value() as i64 {
                    Ok(r::Value::Int((n as i32).into()))
                } else {
                    Err(q::Value::Int(num))
                }
            }
            ("Int8", q::Value::Int(num)) => {
                let n = num.as_i64().ok_or_else(|| q::Value::Int(num.clone()))?;
                Ok(r::Value::Int(n))
            }
            ("String", q::Value::String(s)) => Ok(r::Value::String(s)),
            ("ID", q::Value::String(s)) => Ok(r::Value::String(s)),
            ("ID", q::Value::Int(n)) => Ok(r::Value::String(
                n.as_i64().ok_or(q::Value::Int(n))?.to_string(),
            )),
            ("Bytes", q::Value::String(s)) => Ok(r::Value::String(s)),
            ("BigInt", q::Value::String(s)) => Ok(r::Value::String(s)),
            ("BigInt", q::Value::Int(n)) => Ok(r::Value::String(
                n.as_i64().ok_or(q::Value::Int(n))?.to_string(),
            )),
            (_, v) => Err(v),
        }
    }
}

/// On error, the `value` is returned as `Err(value)`.
fn coerce_to_definition<'a>(
    value: r::Value,
    definition: &str,
    resolver: &impl Fn(&str) -> Option<&'a TypeDefinition>,
) -> Result<r::Value, r::Value> {
    match resolver(definition).ok_or_else(|| value.clone())? {
        // Accept enum values if they match a value in the enum type
        TypeDefinition::Enum(t) => value.coerce_enum(t),

        // Try to coerce Scalar values
        TypeDefinition::Scalar(t) => value.coerce_scalar(t),

        // Try to coerce InputObject values
        TypeDefinition::InputObject(t) => match value {
            r::Value::Object(object) => {
                let object_for_error = r::Value::Object(object.clone());
                let mut coerced_object = BTreeMap::new();
                for (name, value) in object {
                    let def = t
                        .fields
                        .iter()
                        .find(|f| f.name == &*name)
                        .ok_or_else(|| object_for_error.clone())?;
                    coerced_object.insert(
                        name.clone(),
                        match coerce_input_value(Some(value), def, resolver) {
                            Err(_) | Ok(None) => return Err(object_for_error),
                            Ok(Some(v)) => v,
                        },
                    );
                }
                Ok(r::Value::object(coerced_object))
            }
            _ => Err(value),
        },

        // Everything else remains unimplemented
        _ => Err(value),
    }
}

/// Coerces an argument into a GraphQL value.
///
/// `Ok(None)` happens when no value is found for a nullable type.
pub(crate) fn coerce_input_value<'a>(
    mut value: Option<r::Value>,
    def: &InputValue,
    resolver: &impl Fn(&str) -> Option<&'a TypeDefinition>,
) -> Result<Option<r::Value>, QueryExecutionError> {
    // Use the default value if necessary and present.
    value = match value {
        Some(value) => Some(value),
        None => def
            .default_value
            .clone()
            .map(r::Value::try_from)
            .transpose()
            .map_err(|value| {
                QueryExecutionError::Panic(format!(
                    "internal error: failed to convert default value {:?}",
                    value
                ))
            })?,
    };

    // Extract value, checking for null or missing.
    let value = match value {
        None => {
            return if schema::ast::is_non_null_type(&def.value_type) {
                Err(QueryExecutionError::MissingArgumentError(
                    def.position,
                    def.name.clone(),
                ))
            } else {
                Ok(None)
            };
        }
        Some(value) => value,
    };

    Ok(Some(
        coerce_value(value, &def.value_type, resolver).map_err(|val| {
            QueryExecutionError::InvalidArgumentError(def.position, def.name.clone(), val.into())
        })?,
    ))
}

/// On error, the `value` is returned as `Err(value)`.
pub(crate) fn coerce_value<'a>(
    value: r::Value,
    ty: &Type,
    resolver: &impl Fn(&str) -> Option<&'a TypeDefinition>,
) -> Result<r::Value, r::Value> {
    match (ty, value) {
        // Null values cannot be coerced into non-null types.
        (Type::NonNullType(_), r::Value::Null) => Err(r::Value::Null),

        // Non-null values may be coercible into non-null types
        (Type::NonNullType(_), val) => {
            // We cannot bind `t` in the pattern above because "binding by-move and by-ref in the
            // same pattern is unstable". Refactor this and the others when Rust fixes this.
            let t = match ty {
                Type::NonNullType(ty) => ty,
                _ => unreachable!(),
            };
            coerce_value(val, t, resolver)
        }

        // Nullable types can be null.
        (_, r::Value::Null) => Ok(r::Value::Null),

        // Resolve named types, then try to coerce the value into the resolved type
        (Type::NamedType(_), val) => {
            let name = match ty {
                Type::NamedType(name) => name,
                _ => unreachable!(),
            };
            coerce_to_definition(val, name, resolver)
        }

        // List values are coercible if their values are coercible into the
        // inner type.
        (Type::ListType(_), r::Value::List(values)) => {
            let t = match ty {
                Type::ListType(ty) => ty,
                _ => unreachable!(),
            };
            let mut coerced_values = vec![];

            // Coerce the list values individually
            for value in values {
                coerced_values.push(coerce_value(value, t, resolver)?);
            }

            Ok(r::Value::List(coerced_values))
        }

        // Otherwise the list type is not coercible.
        (Type::ListType(_), value) => Err(value),
    }
}

#[cfg(test)]
mod tests {
    use graph::prelude::r::Value;
    use graphql_parser::schema::{EnumType, EnumValue, ScalarType, TypeDefinition};
    use graphql_parser::Pos;

    use super::coerce_to_definition;

    #[test]
    fn coercion_using_enum_type_definitions_is_correct() {
        let enum_type = TypeDefinition::Enum(EnumType {
            name: "Enum".to_string(),
            description: None,
            directives: vec![],
            position: Pos::default(),
            values: vec![EnumValue {
                name: "ValidVariant".to_string(),
                position: Pos::default(),
                description: None,
                directives: vec![],
            }],
        });
        let resolver = |_: &str| Some(&enum_type);

        // We can coerce from Value::Enum -> TypeDefinition::Enum if the variant is valid
        assert_eq!(
            coerce_to_definition(Value::Enum("ValidVariant".to_string()), "", &resolver,),
            Ok(Value::Enum("ValidVariant".to_string()))
        );

        // We cannot coerce from Value::Enum -> TypeDefinition::Enum if the variant is invalid
        assert!(
            coerce_to_definition(Value::Enum("InvalidVariant".to_string()), "", &resolver,)
                .is_err()
        );

        // We also support going from Value::String -> TypeDefinition::Scalar(Enum)
        assert_eq!(
            coerce_to_definition(Value::String("ValidVariant".to_string()), "", &resolver,),
            Ok(Value::Enum("ValidVariant".to_string())),
        );

        // But we don't support invalid variants
        assert!(
            coerce_to_definition(Value::String("InvalidVariant".to_string()), "", &resolver,)
                .is_err()
        );
    }

    #[test]
    fn coercion_using_boolean_type_definitions_is_correct() {
        let bool_type = TypeDefinition::Scalar(ScalarType {
            name: "Boolean".to_string(),
            description: None,
            directives: vec![],
            position: Pos::default(),
        });
        let resolver = |_: &str| Some(&bool_type);

        // We can coerce from Value::Boolean -> TypeDefinition::Scalar(Boolean)
        assert_eq!(
            coerce_to_definition(Value::Boolean(true), "", &resolver),
            Ok(Value::Boolean(true))
        );
        assert_eq!(
            coerce_to_definition(Value::Boolean(false), "", &resolver),
            Ok(Value::Boolean(false))
        );

        // We don't support going from Value::String -> TypeDefinition::Scalar(Boolean)
        assert!(coerce_to_definition(Value::String("true".to_string()), "", &resolver,).is_err());
        assert!(coerce_to_definition(Value::String("false".to_string()), "", &resolver,).is_err());

        // We don't support going from Value::Float -> TypeDefinition::Scalar(Boolean)
        assert!(coerce_to_definition(Value::Float(1.0), "", &resolver).is_err());
        assert!(coerce_to_definition(Value::Float(0.0), "", &resolver).is_err());
    }

    #[test]
    fn coercion_using_big_decimal_type_definitions_is_correct() {
        let big_decimal_type = TypeDefinition::Scalar(ScalarType::new("BigDecimal".to_string()));
        let resolver = |_: &str| Some(&big_decimal_type);

        // We can coerce from Value::Float -> TypeDefinition::Scalar(BigDecimal)
        assert_eq!(
            coerce_to_definition(Value::Float(23.7), "", &resolver),
            Ok(Value::String("23.7".to_string()))
        );
        assert_eq!(
            coerce_to_definition(Value::Float(-5.879), "", &resolver),
            Ok(Value::String("-5.879".to_string()))
        );

        // We can coerce from Value::String -> TypeDefinition::Scalar(BigDecimal)
        assert_eq!(
            coerce_to_definition(Value::String("23.7".to_string()), "", &resolver,),
            Ok(Value::String("23.7".to_string()))
        );
        assert_eq!(
            coerce_to_definition(Value::String("-5.879".to_string()), "", &resolver,),
            Ok(Value::String("-5.879".to_string())),
        );

        // We can coerce from Value::Int -> TypeDefinition::Scalar(BigDecimal)
        assert_eq!(
            coerce_to_definition(Value::Int(23.into()), "", &resolver),
            Ok(Value::String("23".to_string()))
        );
        assert_eq!(
            coerce_to_definition(Value::Int((-5_i32).into()), "", &resolver,),
            Ok(Value::String("-5".to_string())),
        );

        // We don't support going from Value::Boolean -> TypeDefinition::Scalar(Boolean)
        assert!(coerce_to_definition(Value::Boolean(true), "", &resolver).is_err());
        assert!(coerce_to_definition(Value::Boolean(false), "", &resolver).is_err());
    }

    #[test]
    fn coercion_using_string_type_definitions_is_correct() {
        let string_type = TypeDefinition::Scalar(ScalarType::new("String".to_string()));
        let resolver = |_: &str| Some(&string_type);

        // We can coerce from Value::String -> TypeDefinition::Scalar(String)
        assert_eq!(
            coerce_to_definition(Value::String("foo".to_string()), "", &resolver,),
            Ok(Value::String("foo".to_string()))
        );
        assert_eq!(
            coerce_to_definition(Value::String("bar".to_string()), "", &resolver,),
            Ok(Value::String("bar".to_string()))
        );

        // We don't support going from Value::Boolean -> TypeDefinition::Scalar(String)
        assert!(coerce_to_definition(Value::Boolean(true), "", &resolver).is_err());
        assert!(coerce_to_definition(Value::Boolean(false), "", &resolver).is_err());

        // We don't support going from Value::Float -> TypeDefinition::Scalar(String)
        assert!(coerce_to_definition(Value::Float(23.7), "", &resolver).is_err());
        assert!(coerce_to_definition(Value::Float(-5.879), "", &resolver).is_err());
    }

    #[test]
    fn coercion_using_id_type_definitions_is_correct() {
        let string_type = TypeDefinition::Scalar(ScalarType::new("ID".to_owned()));
        let resolver = |_: &str| Some(&string_type);

        // We can coerce from Value::String -> TypeDefinition::Scalar(ID)
        assert_eq!(
            coerce_to_definition(Value::String("foo".to_string()), "", &resolver,),
            Ok(Value::String("foo".to_string()))
        );
        assert_eq!(
            coerce_to_definition(Value::String("bar".to_string()), "", &resolver,),
            Ok(Value::String("bar".to_string()))
        );

        // And also from Value::Int
        assert_eq!(
            coerce_to_definition(Value::Int(1234.into()), "", &resolver),
            Ok(Value::String("1234".to_string()))
        );

        // We don't support going from Value::Boolean -> TypeDefinition::Scalar(ID)
        assert!(coerce_to_definition(Value::Boolean(true), "", &resolver).is_err());

        assert!(coerce_to_definition(Value::Boolean(false), "", &resolver).is_err());

        // We don't support going from Value::Float -> TypeDefinition::Scalar(ID)
        assert!(coerce_to_definition(Value::Float(23.7), "", &resolver).is_err());
        assert!(coerce_to_definition(Value::Float(-5.879), "", &resolver).is_err());
    }

    #[test]
    fn coerce_big_int_scalar() {
        let big_int_type = TypeDefinition::Scalar(ScalarType::new("BigInt".to_string()));
        let resolver = |_: &str| Some(&big_int_type);

        // We can coerce from Value::String -> TypeDefinition::Scalar(BigInt)
        assert_eq!(
            coerce_to_definition(Value::String("1234".to_string()), "", &resolver,),
            Ok(Value::String("1234".to_string()))
        );

        // And also from Value::Int
        assert_eq!(
            coerce_to_definition(Value::Int(1234.into()), "", &resolver),
            Ok(Value::String("1234".to_string()))
        );
        assert_eq!(
            coerce_to_definition(Value::Int((-1234_i32).into()), "", &resolver,),
            Ok(Value::String("-1234".to_string()))
        );
    }

    #[test]
    fn coerce_int8_scalar() {
        let int8_type = TypeDefinition::Scalar(ScalarType::new("Int8".to_string()));
        let resolver = |_: &str| Some(&int8_type);

        assert_eq!(
            coerce_to_definition(Value::Int(1234.into()), "", &resolver),
            Ok(Value::String("1234".to_string()))
        );
        assert_eq!(
            coerce_to_definition(Value::Int((-1234_i32).into()), "", &resolver,),
            Ok(Value::String("-1234".to_string()))
        );
    }

    #[test]
    fn coerce_bytes_scalar() {
        let bytes_type = TypeDefinition::Scalar(ScalarType::new("Bytes".to_string()));
        let resolver = |_: &str| Some(&bytes_type);

        // We can coerce from Value::String -> TypeDefinition::Scalar(Bytes)
        assert_eq!(
            coerce_to_definition(Value::String("0x21f".to_string()), "", &resolver,),
            Ok(Value::String("0x21f".to_string()))
        );
    }

    #[test]
    fn coerce_int_scalar() {
        let int_type = TypeDefinition::Scalar(ScalarType::new("Int".to_string()));
        let resolver = |_: &str| Some(&int_type);

        assert_eq!(
            coerce_to_definition(Value::Int(13289123.into()), "", &resolver,),
            Ok(Value::Int(13289123.into()))
        );
        assert_eq!(
            coerce_to_definition(Value::Int((-13289123_i32).into()), "", &resolver,),
            Ok(Value::Int((-13289123_i32).into()))
        );
    }
}
