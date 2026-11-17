//! Props checks for built-in JSX tags.
//!
//! Specs are generated from `bubble-schema`. Colors are CSS strings or
//! `0xAARRGGBB`. Lengths are numbers, percent strings, or `"auto"`.
//! Expressions that are not literals stay unchecked.

use std::collections::HashMap;
use std::sync::LazyLock;

use oxc::ast::ast::*;

include!(concat!(env!("OUT_DIR"), "/prim_spec.rs"));

pub(crate) fn infer_prop_type(value: &JSXAttributeValue<'_>) -> PropType {
    match value {
        JSXAttributeValue::StringLiteral(literal) => {
            PropType::StrLit(literal.value.as_str().unwrap_or("").to_string())
        }
        JSXAttributeValue::ExpressionContainer(container) => {
            let Some(expr) = container.expression.as_expression() else {
                return PropType::Unknown;
            };
            match expr {
                Expression::NumericLiteral(_) => PropType::Number,
                Expression::BooleanLiteral(_) => PropType::Boolean,
                Expression::StringLiteral(literal) => {
                    PropType::StrLit(literal.value.as_str().unwrap_or("").to_string())
                }
                Expression::ArrayExpression(_) => PropType::Array(Box::new(PropType::Unknown)),
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                    PropType::Function
                }
                _ => PropType::Unknown,
            }
        }
        _ => PropType::Unknown,
    }
}

pub(crate) fn prop_types_compatible(actual: &PropType, expected: &PropType) -> bool {
    match (actual, expected) {
        (PropType::Unknown, _) | (_, PropType::Unknown) => true,
        (a, b) if a == b => true,
        (PropType::StrLit(_), PropType::String | PropType::Color | PropType::Len) => true,
        (PropType::StrLit(value), PropType::Union(variants)) => variants.contains(&value.as_str()),
        (PropType::String, PropType::Union(variants)) => {
            variants.iter().any(|&v| v != "number" && v != "boolean")
        }
        (PropType::Number, PropType::Color | PropType::Len) => true,
        (PropType::Number, PropType::Union(variants)) => {
            variants.contains(&"number") || variants.contains(&"number[]")
        }
        (PropType::Array(_), PropType::Union(variants)) => {
            variants.iter().any(|variant| variant.contains("[]"))
        }
        (PropType::Function, PropType::Function) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prim_spec_matches_schema() {
        let schema = bubble_schema::bubble_shell_schema();
        for prim in schema.primitives() {
            let spec =
                prim_spec(prim.name).unwrap_or_else(|| panic!("prim_spec missing {}", prim.name));
            assert_eq!(
                spec.allowed.len(),
                prim.props.len(),
                "prop count mismatch for {}",
                prim.name
            );
            let required = prim.required_props();
            assert_eq!(
                spec.required,
                required.as_slice(),
                "required props mismatch for {}",
                prim.name
            );
            for prop in &prim.props {
                let actual = spec
                    .allowed
                    .get(prop.name)
                    .unwrap_or_else(|| panic!("missing {} on {}", prop.name, prim.name));
                assert_eq!(
                    actual,
                    &expect_compiler(&prop.ty),
                    "{} on {}",
                    prop.name,
                    prim.name
                );
            }
        }
    }

    fn expect_compiler(ty: &bubble_schema::PropType) -> PropType {
        use bubble_schema::PropType as SchemaType;
        match ty {
            SchemaType::Color => PropType::Color,
            SchemaType::Len => PropType::Len,
            SchemaType::Number => PropType::Number,
            SchemaType::String => PropType::String,
            SchemaType::Boolean => PropType::Boolean,
            SchemaType::Ref | SchemaType::Unknown => PropType::Unknown,
            SchemaType::EventFn(_) | SchemaType::Func(_) => PropType::Function,
            SchemaType::Edges => PropType::Union(vec!["number", "number[]"]),
            SchemaType::Array(inner) => PropType::Array(Box::new(expect_compiler(inner))),
            SchemaType::Union(parts) => PropType::Union(
                parts
                    .iter()
                    .map(|part| match part {
                        SchemaType::StrLit(text) => *text,
                        other => {
                            panic!("compiler union only supports string literals, got {other:?}")
                        }
                    })
                    .collect(),
            ),
            other => panic!("not a JSX prop type: {other:?}"),
        }
    }
}
