use crate::{
    EventSpec, Field, FnType, GestureSpec, Method, Param, PlatformApi, PrimitiveSpec, PropDef,
    PropType, Schema,
};

/// Generated half of `bubble-shell.d.ts`. Hand-written generics (`Signal`,
/// `Ref`, `SharedValue`, control flow, styles, worklets) are prepended by
/// the CLI.
pub fn generate(schema: &Schema) -> String {
    let mut out = String::new();
    out.push_str(
        "// ----------------------------------------------------------------------------\n",
    );
    out.push_str("// Generated from bubble-schema\n");
    out.push_str(
        "// ----------------------------------------------------------------------------\n\n",
    );
    out.push_str("/** CSS color (`#rgb`, `oklch()`, named) or `0xAARRGGBB`. */\n");
    out.push_str("export type Color = string | number;\n");
    out.push_str("/** Point length, percent string, or `\"auto\"`. */\n");
    out.push_str("export type Length = number | string;\n\n");

    for event in schema.events() {
        push_event(&mut out, event);
    }
    for prim in schema.primitives() {
        push_primitive(&mut out, prim);
    }
    for gesture in schema.gestures() {
        push_gesture(&mut out, gesture);
    }
    if !schema.platform().is_empty() {
        out.push_str(
            "// ----------------------------------------------------------------------------\n",
        );
        out.push_str("// Platform APIs\n");
        out.push_str(
            "// ----------------------------------------------------------------------------\n\n",
        );
        for api in schema.platform() {
            push_platform(&mut out, api);
        }
    }
    out
}

fn push_event(out: &mut String, event: &EventSpec) {
    out.push_str("export interface ");
    out.push_str(event.name);
    out.push_str(" {\n");
    for field in &event.fields {
        out.push_str("  ");
        out.push_str(field.name);
        out.push_str(": ");
        out.push_str(&prop_type_to_ts(&field.ty));
        out.push_str(";\n");
    }
    out.push_str("}\n\n");
}

fn push_primitive(out: &mut String, prim: &PrimitiveSpec) {
    let props_name = format!("{}Props", prim.name);
    out.push_str("export interface ");
    out.push_str(&props_name);
    out.push_str(" {\n");
    for prop in &prim.props {
        push_prop(out, prop);
    }
    out.push_str("}\n");
    out.push_str("export const ");
    out.push_str(prim.name);
    out.push_str(": (props: ");
    out.push_str(&props_name);
    out.push_str(") => void;\n\n");
}

fn push_gesture(out: &mut String, gesture: &GestureSpec) {
    let props_name = format!("{}Props", gesture.name);
    out.push_str("export interface ");
    out.push_str(&props_name);
    out.push_str(" {\n");
    for prop in &gesture.props {
        push_prop(out, prop);
    }
    for handler in &gesture.events {
        out.push_str("  ");
        out.push_str(handler.name);
        out.push_str("?: (event: ");
        out.push_str(handler.event);
        out.push_str(") => void;\n");
    }
    out.push_str("}\n");
    out.push_str("export const ");
    out.push_str(gesture.name);
    out.push_str(": (props: ");
    out.push_str(&props_name);
    out.push_str(") => void;\n\n");
}

fn push_prop(out: &mut String, prop: &PropDef) {
    out.push_str("  ");
    out.push_str(prop.name);
    if !prop.required {
        out.push('?');
    }
    out.push_str(": ");
    out.push_str(&prop_to_ts(prop));
    out.push_str(";\n");
}

fn push_platform(out: &mut String, api: &PlatformApi) {
    match api {
        PlatformApi::Object { name, methods } => {
            out.push_str("export const ");
            out.push_str(name);
            out.push_str(": {\n");
            for method in methods {
                push_method(out, method);
            }
            out.push_str("};\n\n");
        }
        PlatformApi::Function { name, params, ret } => {
            out.push_str("export function ");
            out.push_str(name);
            out.push('(');
            out.push_str(&render_params(params));
            out.push_str("): ");
            out.push_str(&prop_type_to_ts(ret));
            out.push_str(";\n\n");
        }
    }
}

fn push_method(out: &mut String, method: &Method) {
    out.push_str("  ");
    out.push_str(method.name);
    out.push('(');
    out.push_str(&render_params(&method.params));
    out.push_str("): ");
    out.push_str(&prop_type_to_ts(&method.ret));
    out.push_str(";\n");
}

fn prop_to_ts(prop: &PropDef) -> String {
    let base = prop_type_to_ts(&prop.ty);
    if prop.ty.is_dynamic_eligible() {
        let wrapped = if base.contains('|') {
            format!("({base})")
        } else {
            base.clone()
        };
        format!("{base} | Ref<{wrapped}> | SharedValue<{wrapped}>")
    } else {
        base
    }
}

fn prop_type_to_ts(ty: &PropType) -> String {
    match ty {
        PropType::Color => "Color".to_string(),
        PropType::Len => "Length".to_string(),
        PropType::Number => "number".to_string(),
        PropType::String => "string".to_string(),
        PropType::Boolean => "boolean".to_string(),
        PropType::Null => "null".to_string(),
        PropType::Unknown => "unknown".to_string(),
        PropType::Object => "object".to_string(),
        PropType::Void => "void".to_string(),
        PropType::Ref => "Ref".to_string(),
        PropType::StrLit(text) => format!("\"{text}\""),
        PropType::Union(parts) => parts
            .iter()
            .map(prop_type_to_ts)
            .collect::<Vec<_>>()
            .join(" | "),
        PropType::Array(inner) => {
            let inner = prop_type_to_ts(inner);
            if inner.contains('|') || inner.contains(' ') {
                format!("({inner})[]")
            } else {
                format!("{inner}[]")
            }
        }
        PropType::Promise(inner) => format!("Promise<{}>", prop_type_to_ts(inner)),
        PropType::Edges => "number | number[]".to_string(),
        PropType::OneOrArray(variants) => {
            let lits = variants
                .iter()
                .map(|variant| format!("\"{variant}\""))
                .collect::<Vec<_>>()
                .join(" | ");
            format!("{lits} | ({lits})[]")
        }
        PropType::EventFn(name) => format!("(event: {name}) => void"),
        PropType::Func(sig) => render_fn(sig),
        PropType::ObjectLit(fields) => render_object(fields),
    }
}

fn render_fn(sig: &FnType) -> String {
    format!(
        "({}) => {}",
        render_params(&sig.params),
        prop_type_to_ts(&sig.ret)
    )
}

fn render_object(fields: &[Field]) -> String {
    let body = fields
        .iter()
        .map(|field| {
            let optional = if field.optional { "?" } else { "" };
            format!("{}{optional}: {}", field.name, prop_type_to_ts(&field.ty))
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("{{ {body} }}")
}

fn render_params(params: &[Param]) -> String {
    params
        .iter()
        .map(|param| {
            let optional = if param.optional { "?" } else { "" };
            format!("{}{optional}: {}", param.name, prop_type_to_ts(&param.ty))
        })
        .collect::<Vec<_>>()
        .join(", ")
}
