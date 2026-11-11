//! One pass from TSX function components to `bubble` renderer calls.
//!
//! Static props are emitted once. A text child that reads a signal becomes a
//! thunk passed to `text`, which the runtime registers as a binding. There is
//! no template IR.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPatternKind, Expression, FunctionBody, JSXAttributeItem, JSXAttributeValue, JSXChild,
    JSXElement, JSXElementName, Program, Statement,
};
use oxc_parser::Parser;
use oxc_span::SourceType;

pub fn compile(source: &str) -> Result<String, String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::tsx()).parse();
    if !parsed.errors.is_empty() {
        let message = parsed
            .errors
            .iter()
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(message);
    }
    emit_program(&parsed.program)
}

fn emit_program(program: &Program<'_>) -> Result<String, String> {
    let mut out = String::from("import { signal, view, text, mount } from \"../runtime.js\";\n\n");
    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(_) => {}
            Statement::FunctionDeclaration(func) => {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .ok_or("function is missing a name")?;
                let Some(body) = &func.body else {
                    return Err(format!("{name} has no body"));
                };
                out.push_str("function ");
                out.push_str(name);
                out.push_str("() {\n");
                emit_body(&mut out, body, 1)?;
                out.push_str("}\n\n");
            }
            Statement::ExpressionStatement(stmt) => {
                emit_mount(&mut out, &stmt.expression)?;
                out.push('\n');
            }
            other => return Err(format!("unsupported statement {}", other_name(other))),
        }
    }
    Ok(out)
}

fn emit_mount(out: &mut String, expr: &Expression<'_>) -> Result<(), String> {
    let Expression::CallExpression(call) = expr else {
        return Err("top-level expression must be mount(...)".into());
    };
    let Expression::Identifier(name) = &call.callee else {
        return Err("top-level call must be mount".into());
    };
    if name.name != "mount" {
        return Err(format!("unexpected top-level call {}", name.name));
    }
    if call.arguments.len() != 1 {
        return Err("mount takes one component".into());
    }
    let arg = call.arguments[0].as_expression().ok_or("mount argument is not an expression")?;
    match arg {
        Expression::Identifier(id) => {
            out.push_str("mount(");
            out.push_str(&id.name);
            out.push_str(");");
        }
        Expression::ArrowFunctionExpression(_) => {
            return Err("pass a named component to mount".into());
        }
        _ => return Err("mount argument must be a component function".into()),
    }
    Ok(())
}

fn emit_body(out: &mut String, body: &FunctionBody<'_>, indent: usize) -> Result<(), String> {
    for stmt in &body.statements {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                for item in &decl.declarations {
                    let BindingPatternKind::BindingIdentifier(name) = &item.id.kind else {
                        return Err("only plain const bindings are supported".into());
                    };
                    let init = item.init.as_ref().ok_or("const is missing an initializer")?;
                    pad(out, indent);
                    out.push_str("const ");
                    out.push_str(&name.name);
                    out.push_str(" = ");
                    emit_expr(out, init)?;
                    out.push_str(";\n");
                }
            }
            Statement::ReturnStatement(ret) => {
                let value = ret.argument.as_ref().ok_or("return is missing a value")?;
                pad(out, indent);
                out.push_str("return ");
                emit_expr(out, value)?;
                out.push_str(";\n");
            }
            Statement::ExpressionStatement(stmt) => {
                pad(out, indent);
                emit_expr(out, &stmt.expression)?;
                out.push_str(";\n");
            }
            _ => return Err("unsupported statement inside a component".into()),
        }
    }
    Ok(())
}

fn emit_expr(out: &mut String, expr: &Expression<'_>) -> Result<(), String> {
    match expr {
        Expression::JSXElement(element) => emit_jsx(out, element),
        Expression::Identifier(id) => {
            out.push_str(&id.name);
            Ok(())
        }
        Expression::NumericLiteral(lit) => {
            out.push_str(&lit.value.to_string());
            Ok(())
        }
        Expression::BooleanLiteral(lit) => {
            out.push_str(if lit.value { "true" } else { "false" });
            Ok(())
        }
        Expression::StringLiteral(lit) => {
            out.push_str(&js_string(&lit.value));
            Ok(())
        }
        Expression::NullLiteral(_) => {
            out.push_str("null");
            Ok(())
        }
        Expression::CallExpression(call) => {
            emit_expr(out, &call.callee)?;
            out.push('(');
            for (index, arg) in call.arguments.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                let expr = arg.as_expression().ok_or("spread arguments are not supported")?;
                emit_expr(out, expr)?;
            }
            out.push(')');
            Ok(())
        }
        Expression::StaticMemberExpression(member) => {
            emit_expr(out, &member.object)?;
            out.push('.');
            out.push_str(&member.property.name);
            Ok(())
        }
        Expression::ComputedMemberExpression(_) => Err("computed member access is not supported".into()),
        Expression::ArrowFunctionExpression(arrow) => {
            out.push('(');
            for (index, param) in arrow.params.items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                let BindingPatternKind::BindingIdentifier(name) = &param.pattern.kind else {
                    return Err("arrow parameters must be plain identifiers".into());
                };
                out.push_str(&name.name);
            }
            out.push_str(") => ");
            if arrow.expression {
                let Some(Statement::ExpressionStatement(stmt)) = arrow.body.statements.first() else {
                    return Err("arrow expression body is missing".into());
                };
                emit_expr(out, &stmt.expression)?;
            } else {
                out.push_str("{\n");
                emit_body(out, &arrow.body, 1)?;
                out.push('}');
            }
            Ok(())
        }
        Expression::BinaryExpression(binary) => {
            emit_expr(out, &binary.left)?;
            out.push(' ');
            out.push_str(binary.operator.as_str());
            out.push(' ');
            emit_expr(out, &binary.right)?;
            Ok(())
        }
        Expression::ParenthesizedExpression(paren) => {
            out.push('(');
            emit_expr(out, &paren.expression)?;
            out.push(')');
            Ok(())
        }
        Expression::ArrayExpression(array) => {
            out.push('[');
            for (index, item) in array.elements.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                let Some(expr) = item.as_expression() else {
                    return Err("holes and spreads in arrays are not supported".into());
                };
                emit_expr(out, expr)?;
            }
            out.push(']');
            Ok(())
        }
        Expression::TemplateLiteral(_) => Err("template literals are not supported; use +".into()),
        other => Err(format!("unsupported expression {}", expr_name(other))),
    }
}

fn emit_jsx(out: &mut String, element: &JSXElement<'_>) -> Result<(), String> {
    let name = jsx_name(&element.opening_element.name)?;
    if let Some(component) = name.strip_prefix("component:") {
        out.push_str(component);
        out.push_str("()");
        return Ok(());
    }
    let text = name == "Text";
    out.push_str(if text { "text" } else { "view" });
    out.push_str("({");
    let mut wrote = false;
    for attr in &element.opening_element.attributes {
        let JSXAttributeItem::Attribute(attr) = attr else {
            return Err("JSX spread is not supported".into());
        };
        let key = attr.name.as_identifier().ok_or("JSX names must be identifiers")?;
        if wrote {
            out.push_str(", ");
        }
        wrote = true;
        out.push_str(&key.name);
        out.push_str(": ");
        match &attr.value {
            None => out.push_str("true"),
            Some(JSXAttributeValue::StringLiteral(lit)) => out.push_str(&js_string(&lit.value)),
            Some(JSXAttributeValue::ExpressionContainer(container)) => {
                let Some(expr) = container.expression.as_expression() else {
                    return Err("empty JSX expression".into());
                };
                emit_expr(out, expr)?;
            }
            Some(_) => return Err("unsupported JSX attribute value".into()),
        }
    }
    out.push_str("}, ");
    if text {
        emit_text_content(out, &element.children)?;
    } else {
        out.push_str("() => {\n");
        for child in &element.children {
            match child {
                JSXChild::Element(child) => {
                    out.push_str("  ");
                    emit_jsx(out, child)?;
                    out.push_str(";\n");
                }
                JSXChild::Text(text) if text.value.trim().is_empty() => {}
                JSXChild::ExpressionContainer(_) => {
                    return Err("expression children belong on Text".into());
                }
                JSXChild::Text(_) => return Err("text nodes belong on Text".into()),
                _ => return Err("unsupported JSX child".into()),
            }
        }
        out.push('}');
    }
    out.push(')');
    Ok(())
}

fn emit_text_content(out: &mut String, children: &[JSXChild<'_>]) -> Result<(), String> {
    let mut parts: Vec<&JSXChild<'_>> = Vec::new();
    for child in children {
        match child {
            JSXChild::Text(text) if text.value.trim().is_empty() => {}
            other => parts.push(other),
        }
    }
    let dynamic = parts.iter().any(|child| matches!(child, JSXChild::ExpressionContainer(_)));
    if !dynamic {
        let mut text = String::new();
        for child in parts {
            let JSXChild::Text(node) = child else {
                return Err("Text children must be text or expressions".into());
            };
            text.push_str(&node.value);
        }
        out.push_str(&js_string(&text));
        return Ok(());
    }
    out.push_str("() => ");
    let mut first = true;
    for child in parts {
        if !first {
            out.push_str(" + ");
        }
        first = false;
        match child {
            JSXChild::Text(node) => out.push_str(&js_string(&node.value)),
            JSXChild::ExpressionContainer(container) => {
                let Some(expr) = container.expression.as_expression() else {
                    return Err("empty JSX expression".into());
                };
                emit_expr(out, expr)?;
            }
            _ => return Err("Text children must be text or expressions".into()),
        }
    }
    if first {
        out.push_str("\"\"");
    }
    Ok(())
}

fn jsx_name(name: &JSXElementName<'_>) -> Result<String, String> {
    let raw = match name {
        JSXElementName::Identifier(id) => id.name.as_str(),
        JSXElementName::IdentifierReference(id) => id.name.as_str(),
        _ => return Err("only identifier JSX tags are supported".into()),
    };
    match raw {
        "View" => Ok("View".into()),
        "Text" => Ok("Text".into()),
        other => Ok(format!("component:{other}")),
    }
}

fn js_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn pad(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn other_name(stmt: &Statement<'_>) -> &'static str {
    match stmt {
        Statement::ImportDeclaration(_) => "import",
        Statement::FunctionDeclaration(_) => "function",
        Statement::ExpressionStatement(_) => "expression",
        Statement::VariableDeclaration(_) => "variable",
        Statement::ReturnStatement(_) => "return",
        _ => "statement",
    }
}

fn expr_name(expr: &Expression<'_>) -> &'static str {
    match expr {
        Expression::JSXElement(_) => "jsx",
        Expression::Identifier(_) => "identifier",
        Expression::CallExpression(_) => "call",
        _ => "expression",
    }
}
