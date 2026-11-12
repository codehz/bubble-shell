//! Module compiler for Bubble fixtures.
//!
//! One pass over a parsed module:
//!
//! 1. The Oxc transformer strips TypeScript types. JSX is left untouched.
//! 2. JSX elements are rewritten in the AST to `bubble:rt` calls.
//! 3. Oxc codegen prints a single ES module.
//!
//! Arbitrary statements, expressions, imports, and exports survive untouched.
//! The compiler only rejects the JSX shapes listed in `docs/07-js-bundling-plan.md`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use oxc::allocator::{Allocator, ArenaBox, ArenaVec, TakeIn};
use oxc::ast::ast::*;
use oxc::ast::builder::AstBuilder;
use oxc::ast_visit::{Visit, VisitMut};
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::{GetSpan, SourceType, Span, SPAN};
use oxc::str::{Ident, Str};
use oxc::syntax::scope::ScopeFlags;
use oxc::transformer::{JsxOptions, TransformOptions, Transformer};

/// The runtime module injected into every compiled module.
const RT_MODULE: &str = "bubble:rt";
/// The author-facing module. Imports from it are pruned once JSX is lowered.
const SHELL_MODULE: &str = "bubble-shell";
/// The development-only hot runtime.
const HOT_MODULE: &str = "bubble:hot";

const RT_PRIMITIVE: &str = "__bubble_rt_primitive";
const RT_COMPONENT: &str = "__bubble_rt_component";
const RT_DYNAMIC: &str = "__bubble_rt_dynamic";
const RT_SHOW: &str = "__bubble_rt_show";
const RT_FOR_EACH: &str = "__bubble_rt_forEach";
const RT_INDEX_EACH: &str = "__bubble_rt_indexEach";
const RT_SWITCH: &str = "__bubble_rt_switchFlow";

const HOT_COMPONENT: &str = "__bubble_hot_component";
const HOT_KEEP: &str = "__bubble_hot_keep";
const HOT_MOUNT: &str = "__bubble_hot_mount";

const PRIMITIVES: &[&str] = &[
    "View",
    "Text",
    "Image",
    "TextField",
    "TapGesture",
    "DoubleTapGesture",
    "LongPressGesture",
    "PanGesture",
    "TransformGesture",
];

/// Options for [`compile_module`].
#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// Emit a module-level source map. Release builds leave this off.
    pub source_map: bool,
    /// Hot-module key for this module. Set only by `bubble dev`, which wraps
    /// components, top-level signals, and the mount call for state-preserving
    /// updates.
    pub hot: Option<String>,
}

/// Output of [`compile_module`].
#[derive(Debug, Clone, Default)]
pub struct CompiledModule {
    /// The compiled ES module.
    pub code: String,
    /// Source map JSON when [`CompileOptions::source_map`] is set.
    pub map: Option<String>,
}

/// Compile one module. `name` selects the source type and the source map entry.
pub fn compile_module(
    name: &str,
    source: &str,
    options: &CompileOptions,
) -> Result<CompiledModule, String> {
    let source_type = source_type_for(name)?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(render_diagnostics(name, source, &parsed.diagnostics));
    }
    let mut program = parsed.program;

    let semantic = SemanticBuilder::new().build(&program);
    let scoping = semantic.semantic.into_scoping();
    let transformed = Transformer::new(
        &allocator,
        Path::new(name),
        &TransformOptions {
            jsx: JsxOptions::disable(),
            ..TransformOptions::default()
        },
    )
    .build_with_scoping(scoping, &mut program);
    if !transformed.diagnostics.is_empty() {
        return Err(render_diagnostics(name, source, &transformed.diagnostics));
    }

    program
        .body
        .insert(0, runtime_import(&AstBuilder::new(&allocator)));

    let mut lower = Lower {
        allocator: &allocator,
        builder: AstBuilder::new(&allocator),
        name,
        source,
        scopes: Vec::new(),
        errors: Vec::new(),
    };
    lower.visit_program(&mut program);
    if let Some(error) = lower.errors.first() {
        return Err(error.clone());
    }

    if let Some(key) = &options.hot {
        lower.hot_transform(&mut program, key);
    }
    prune_shell_imports(&mut program);

    let codegen = Codegen::new().with_options(CodegenOptions {
        source_map_path: options.source_map.then(|| PathBuf::from(name)),
        ..CodegenOptions::default()
    });
    let output = codegen.build(&program);
    Ok(CompiledModule {
        code: output.code,
        map: output.map.map(|map| map.to_json_string()),
    })
}

fn source_type_for(name: &str) -> Result<SourceType, String> {
    let supported = ["tsx", "ts", "jsx", "js"]
        .iter()
        .any(|ext| name.ends_with(&format!(".{ext}")));
    if !supported {
        return Err(format!(
            "{name}: unsupported module; expected a .tsx, .ts, .jsx, or .js file"
        ));
    }
    SourceType::from_path(Path::new(name))
        .map_err(|_| format!("{name}: unsupported module extension"))
}

fn runtime_import<'a>(builder: &AstBuilder<'a>) -> Statement<'a> {
    const BINDINGS: &[(&str, &str)] = &[
        ("primitive", RT_PRIMITIVE),
        ("component", RT_COMPONENT),
        ("dynamic", RT_DYNAMIC),
        ("show", RT_SHOW),
        ("forEach", RT_FOR_EACH),
        ("indexEach", RT_INDEX_EACH),
        ("switchFlow", RT_SWITCH),
    ];
    let mut specifiers = ArenaVec::new_in(builder);
    for &(imported, local) in BINDINGS {
        let specifier = ImportSpecifier::new(
            SPAN,
            ModuleExportName::IdentifierName(IdentifierName::new(
                SPAN,
                Ident::from(imported),
                builder,
            )),
            BindingIdentifier::new(SPAN, Ident::from(local), builder),
            ImportOrExportKind::Value,
            builder,
        );
        specifiers.push(ImportDeclarationSpecifier::ImportSpecifier(
            ArenaBox::new_in(specifier, builder),
        ));
    }
    Statement::new_import_declaration(
        SPAN,
        Some(specifiers),
        StringLiteral::new(SPAN, Str::from(RT_MODULE), None, builder),
        None,
        None,
        ImportOrExportKind::Value,
        builder,
    )
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

fn render_diagnostics(
    name: &str,
    source: &str,
    diagnostics: &oxc::diagnostics::Diagnostics,
) -> String {
    let mut out = String::new();
    for diagnostic in diagnostics {
        let span = diagnostic
            .labels
            .first()
            .map(|label| Span::new(label.offset() as u32, (label.offset() + label.len()) as u32));
        out.push_str(&error_frame(name, source, span, &diagnostic.message));
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn error_frame(name: &str, source: &str, span: Option<Span>, message: &str) -> String {
    let (line, column, line_text) = match span {
        Some(span) => locate(source, span),
        None => (1, 1, ""),
    };
    let mut out = format!("{name}:{line}:{column}: {message}\n");
    if !line_text.is_empty() {
        out.push_str(&format!("  {line:>4} | {line_text}\n"));
        let carets = span
            .map(|span| {
                let len = (span.end - span.start).max(1) as usize;
                len.min(line_text.len().saturating_sub(column - 1).max(1))
            })
            .unwrap_or(1);
        out.push_str(&format!(
            "       | {}{}\n",
            " ".repeat(column - 1),
            "^".repeat(carets)
        ));
    }
    out
}

fn locate(source: &str, span: Span) -> (usize, usize, &str) {
    let start = (span.start as usize).min(source.len());
    let before = &source[..start];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
    let column = before[line_start..].chars().count() + 1;
    let line_end = source[start..]
        .find('\n')
        .map(|index| start + index)
        .unwrap_or(source.len());
    (line, column, &source[line_start..line_end])
}

// ---------------------------------------------------------------------------
// JSX lowering
// ---------------------------------------------------------------------------

struct Lower<'a> {
    allocator: &'a Allocator,
    builder: AstBuilder<'a>,
    name: &'a str,
    source: &'a str,
    /// Signal bindings per lexical scope, outermost first.
    scopes: Vec<HashSet<String>>,
    errors: Vec<String>,
}

impl<'a> Lower<'a> {
    fn error(&mut self, span: Span, message: impl AsRef<str>) {
        if self.errors.is_empty() {
            self.errors.push(error_frame(
                self.name,
                self.source,
                Some(span),
                message.as_ref(),
            ));
        }
    }

    fn ast_vec<T>(&self) -> ArenaVec<'a, T> {
        ArenaVec::new_in(&self.builder)
    }

    /// Move an arena node out of its box, leaving a dummy behind.
    fn take<T: TakeIn<'a>>(&self, mut boxed: ArenaBox<'a, T>) -> T {
        boxed.take_in(&self.builder)
    }

    fn ident(&self, name: &'a str) -> Expression<'a> {
        Expression::Identifier(ArenaBox::new_in(
            IdentifierReference::new(SPAN, Ident::from(name), &self.builder),
            &self.builder,
        ))
    }

    fn string(&self, value: &'a str, span: Span) -> Expression<'a> {
        Expression::StringLiteral(ArenaBox::new_in(
            StringLiteral::new(span, Str::from(value), None, &self.builder),
            &self.builder,
        ))
    }

    fn null(&self, span: Span) -> Expression<'a> {
        Expression::new_null_literal(span, &self.builder)
    }

    fn call(&self, callee: &'a str, args: Vec<Expression<'a>>, span: Span) -> Expression<'a> {
        let mut arguments = self.ast_vec();
        for arg in args {
            arguments.push(Argument::from(arg));
        }
        Expression::new_call_expression(
            span,
            self.ident(callee),
            None,
            arguments,
            false,
            &self.builder,
        )
    }

    fn arrow(&self, span: Span, body: ArrowFunctionBody<'a>) -> Expression<'a> {
        let params = FormalParameters::boxed(
            span,
            FormalParameterKind::ArrowFormalParameters,
            self.ast_vec::<FormalParameter>(),
            None,
            &self.builder,
        );
        Expression::new_arrow_function_expression(
            span,
            false,
            None,
            params,
            None,
            body,
            &self.builder,
        )
    }

    fn arrow_expr(&self, span: Span, expression: Expression<'a>) -> Expression<'a> {
        self.arrow(span, ArrowFunctionBody::from(expression))
    }

    fn arrow_block(&self, span: Span, statements: ArenaVec<'a, Statement<'a>>) -> Expression<'a> {
        let body =
            FunctionBody::boxed(span, self.ast_vec::<Directive>(), statements, &self.builder);
        self.arrow(span, ArrowFunctionBody::FunctionBody(body))
    }

    fn statement(&self, span: Span, expression: Expression<'a>) -> Statement<'a> {
        Statement::new_expression_statement(span, expression, &self.builder)
    }

    fn object(&self, span: Span, props: Vec<(&'a str, Expression<'a>)>) -> Expression<'a> {
        let mut properties = self.ast_vec();
        for (key, value) in props {
            let name = IdentifierName::new(span, Ident::from(key), &self.builder);
            let key = PropertyKey::StaticIdentifier(ArenaBox::new_in(name, &self.builder));
            let property = ObjectProperty::boxed(
                span,
                PropertyKind::Init,
                key,
                value,
                false,
                false,
                false,
                &self.builder,
            );
            properties.push(ObjectPropertyKind::ObjectProperty(property));
        }
        Expression::new_object_expression(span, properties, &self.builder)
    }

    /// Wrap `expression` in `dynamic(() => ...)` when it reads a local signal.
    fn dynamic(&self, span: Span, expression: Expression<'a>) -> Expression<'a> {
        if !self.reads_signal(&expression) {
            return expression;
        }
        let thunk = self.arrow_expr(span, expression);
        self.call(RT_DYNAMIC, vec![thunk], span)
    }

    fn reads_signal(&self, expression: &Expression<'a>) -> bool {
        let Some(signals) = self.scopes.last() else {
            return false;
        };
        let mut scan = SignalScan {
            signals,
            found: false,
        };
        scan.visit_expression(expression);
        scan.found
    }

    fn push_scope(&mut self, bindings: HashSet<String>) {
        let mut scope = self.scopes.last().cloned().unwrap_or_default();
        scope.extend(bindings);
        self.scopes.push(scope);
    }

    fn lower_element(&mut self, element: JSXElement<'a>) -> Expression<'a> {
        let span = element.span;
        let tag = match &element.opening_element.name {
            JSXElementName::Identifier(id) => self.allocator.alloc_str(id.name.as_str()),
            JSXElementName::IdentifierReference(id) => self.allocator.alloc_str(id.name.as_str()),
            _ => {
                self.error(span, "only identifier JSX tags are supported");
                return self.null(span);
            }
        };
        let JSXElement {
            opening_element,
            children,
            ..
        } = element;
        let JSXOpeningElement { attributes, .. } = self.take(opening_element);

        match tag {
            "Show" => self.lower_show(span, attributes, children),
            "For" => self.lower_for(span, attributes, children),
            "Index" => self.lower_index(span, attributes, children),
            "Switch" => self.lower_switch(span, attributes, children),
            "Match" => {
                self.error(span, "Match must be a child of Switch");
                self.null(span)
            }
            other => {
                let (props, controls) =
                    self.split_attributes(span, attributes, &control_names(other));
                debug_assert!(controls.is_empty());
                let props = props
                    .into_iter()
                    .map(|(key, value)| (key, self.prepare_value(value)))
                    .collect::<Vec<_>>();
                let props = self.object(span, props);
                if PRIMITIVES.contains(&other) {
                    if other == "Text" {
                        let content = self.text_content(span, children);
                        self.call(
                            RT_PRIMITIVE,
                            vec![self.string_of(other), props, content],
                            span,
                        )
                    } else {
                        let body = self.children_block(span, children);
                        self.call(RT_PRIMITIVE, vec![self.string_of(other), props, body], span)
                    }
                } else if other
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
                {
                    let callee = self.ident(other);
                    let mut args = vec![callee, props];
                    if has_children(&children) {
                        args.push(self.children_block(span, children));
                    }
                    self.call(RT_COMPONENT, args, span)
                } else {
                    self.error(
                        span,
                        format!(
                            "unknown primitive <{other}>; use View, Text, TextField, Image, a gesture, or a capitalized component"
                        ),
                    );
                    self.null(span)
                }
            }
        }
    }

    fn string_of(&self, value: &str) -> Expression<'a> {
        let value = self.allocator.alloc_str(value);
        self.string(value, SPAN)
    }

    /// Split control-flow attributes from render props.
    fn split_attributes(
        &mut self,
        span: Span,
        attributes: ArenaVec<'a, JSXAttributeItem<'a>>,
        control: &[&'static str],
    ) -> (
        Vec<(&'a str, Expression<'a>)>,
        Vec<(&'static str, Expression<'a>)>,
    ) {
        let mut props = Vec::new();
        let mut controls = Vec::new();
        for item in attributes {
            let JSXAttributeItem::Attribute(attribute) = item else {
                self.error(span, "JSX spread props are not supported");
                continue;
            };
            let JSXAttribute {
                span: attr_span,
                name,
                value,
                ..
            } = self.take(attribute);
            let key = match name {
                JSXAttributeName::Identifier(id) => id.name.as_str(),
                JSXAttributeName::NamespacedName(_) => {
                    self.error(attr_span, "namespaced JSX attributes are not supported");
                    continue;
                }
            };
            let value = match self.attribute_value(value) {
                Some(value) => value,
                None => continue,
            };
            if let Some(slot) = control.iter().find(|slot| **slot == key) {
                controls.push((*slot, value));
            } else {
                props.push((key, value));
            }
        }
        (props, controls)
    }

    fn attribute_value(&mut self, value: Option<JSXAttributeValue<'a>>) -> Option<Expression<'a>> {
        match value {
            None => Some(Expression::new_boolean_literal(SPAN, true, &self.builder)),
            Some(JSXAttributeValue::StringLiteral(literal)) => {
                Some(Expression::StringLiteral(literal))
            }
            Some(JSXAttributeValue::ExpressionContainer(container)) => {
                let container = self.take(container);
                if !container.expression.is_expression() {
                    self.error(container.span, "empty JSX expression");
                    return None;
                }
                Some(container.expression.into_expression())
            }
            Some(JSXAttributeValue::Element(element)) => {
                let element = self.take(element);
                Some(Expression::JSXElement(ArenaBox::new_in(
                    element,
                    &self.builder,
                )))
            }
            Some(JSXAttributeValue::Fragment(_)) => {
                self.error(SPAN, "JSX attribute values must be strings or expressions");
                None
            }
        }
    }

    fn control<'b>(
        &mut self,
        span: Span,
        controls: &'b mut Vec<(&'static str, Expression<'a>)>,
        key: &str,
    ) -> Option<Expression<'a>> {
        let position = controls.iter().position(|(name, _)| *name == key)?;
        let (_, value) = controls.remove(position);
        let _ = span;
        Some(value)
    }

    fn required_control(
        &mut self,
        span: Span,
        controls: &mut Vec<(&'static str, Expression<'a>)>,
        key: &str,
    ) -> Option<Expression<'a>> {
        let value = self.control(span, controls, key);
        if value.is_none() {
            self.error(span, format!("<{key}> requires a {key} prop"));
        }
        value
    }

    /// Lower a prop value once, then bind it when it reads a signal.
    fn prepare_value(&mut self, expression: Expression<'a>) -> Expression<'a> {
        let is_function = matches!(
            expression,
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
        );
        let span = expression.span();
        let mut expression = expression;
        self.visit_expression(&mut expression);
        if is_function {
            expression
        } else {
            self.dynamic(span, expression)
        }
    }

    fn accessor_value(&mut self, span: Span, mut expression: Expression<'a>) -> Expression<'a> {
        self.visit_expression(&mut expression);
        self.arrow_expr(span, expression)
    }

    fn fallback(&mut self, span: Span, value: Option<Expression<'a>>) -> Expression<'a> {
        match value {
            None => self.null(span),
            Some(Expression::NullLiteral(_)) => self.null(span),
            Some(Expression::JSXElement(element)) => {
                let element = self.take(element);
                let lowered = self.lower_element(element);
                self.arrow_expr(span, lowered)
            }
            Some(_) => {
                self.error(span, "fallback must be JSX or null");
                self.null(span)
            }
        }
    }

    fn lower_show(
        &mut self,
        span: Span,
        attributes: ArenaVec<'a, JSXAttributeItem<'a>>,
        children: ArenaVec<'a, JSXChild<'a>>,
    ) -> Expression<'a> {
        let (_, mut controls) = self.split_attributes(span, attributes, &["when", "fallback"]);
        let when = match self.required_control(span, &mut controls, "when") {
            Some(expr) => self.accessor_value(span, expr),
            None => self.null(span),
        };
        let fallback_value = self.control(span, &mut controls, "fallback");
        let fallback = self.fallback(span, fallback_value);
        let render = self.callback(span, children);
        self.call(RT_SHOW, vec![when, fallback, render], span)
    }

    fn lower_for(
        &mut self,
        span: Span,
        attributes: ArenaVec<'a, JSXAttributeItem<'a>>,
        children: ArenaVec<'a, JSXChild<'a>>,
    ) -> Expression<'a> {
        let (_, mut controls) = self.split_attributes(span, attributes, &["each", "by"]);
        let each = match self.required_control(span, &mut controls, "each") {
            Some(expr) => self.accessor_value(span, expr),
            None => self.null(span),
        };
        let by = match self.required_control(span, &mut controls, "by") {
            Some(expr) => self.prepare_value(expr),
            None => self.null(span),
        };
        let render = self.callback(span, children);
        self.call(RT_FOR_EACH, vec![each, by, render], span)
    }

    fn lower_index(
        &mut self,
        span: Span,
        attributes: ArenaVec<'a, JSXAttributeItem<'a>>,
        children: ArenaVec<'a, JSXChild<'a>>,
    ) -> Expression<'a> {
        let (_, mut controls) = self.split_attributes(span, attributes, &["each"]);
        let each = match self.required_control(span, &mut controls, "each") {
            Some(expr) => self.accessor_value(span, expr),
            None => self.null(span),
        };
        let render = self.callback(span, children);
        self.call(RT_INDEX_EACH, vec![each, render], span)
    }

    fn lower_switch(
        &mut self,
        span: Span,
        attributes: ArenaVec<'a, JSXAttributeItem<'a>>,
        children: ArenaVec<'a, JSXChild<'a>>,
    ) -> Expression<'a> {
        let (_, mut controls) = self.split_attributes(span, attributes, &["fallback"]);
        let fallback_value = self.control(span, &mut controls, "fallback");
        let fallback = self.fallback(span, fallback_value);
        let mut arms = self.ast_vec();
        let mut has_arm = false;
        for child in children {
            match child {
                JSXChild::Text(text) if text.value.trim().is_empty() => {}
                JSXChild::Element(element) => {
                    let element = self.take(element);
                    let element_span = element.span;
                    let tag = match &element.opening_element.name {
                        JSXElementName::Identifier(id) => id.name.as_str(),
                        JSXElementName::IdentifierReference(id) => id.name.as_str(),
                        _ => "",
                    };
                    if tag != "Match" {
                        self.error(element_span, "Switch children must be Match");
                        continue;
                    }
                    let JSXElement {
                        opening_element,
                        children,
                        ..
                    } = element;
                    let JSXOpeningElement { attributes, .. } = self.take(opening_element);
                    let (_, mut controls) =
                        self.split_attributes(element_span, attributes, &["when"]);
                    let when = match self.required_control(element_span, &mut controls, "when") {
                        Some(expr) => self.accessor_value(element_span, expr),
                        None => self.null(element_span),
                    };
                    let render = self.callback(element_span, children);
                    let mut pair = self.ast_vec();
                    pair.push(ArrayExpressionElement::from(when));
                    pair.push(ArrayExpressionElement::from(render));
                    arms.push(ArrayExpressionElement::new_array_expression(
                        element_span,
                        pair,
                        &self.builder,
                    ));
                    has_arm = true;
                }
                _ => self.error(span, "Switch children must be Match"),
            }
        }
        let _ = has_arm;
        let arms = Expression::new_array_expression(span, arms, &self.builder);
        self.call(RT_SWITCH, vec![fallback, arms], span)
    }

    /// The single render callback of a control-flow element.
    fn callback(&mut self, span: Span, children: ArenaVec<'a, JSXChild<'a>>) -> Expression<'a> {
        for child in children {
            match child {
                JSXChild::Text(text) if text.value.trim().is_empty() => {}
                JSXChild::ExpressionContainer(container) => {
                    let container = self.take(container);
                    if !container.expression.is_expression() {
                        continue;
                    }
                    let mut expression = container.expression.into_expression();
                    if !matches!(expression, Expression::ArrowFunctionExpression(_)) {
                        self.error(span, "control flow child must be an arrow function");
                        return self.null(span);
                    }
                    self.visit_expression(&mut expression);
                    return expression;
                }
                _ => {}
            }
        }
        self.error(span, "control flow child must be an arrow function");
        self.null(span)
    }

    /// Build `() => { <render calls> }` for a primitive or component.
    fn children_block(
        &mut self,
        span: Span,
        children: ArenaVec<'a, JSXChild<'a>>,
    ) -> Expression<'a> {
        let mut statements = self.ast_vec();
        for child in children {
            match child {
                JSXChild::Text(text) if text.value.trim().is_empty() => {}
                JSXChild::Text(text) => {
                    self.error(text.span, "text nodes belong inside Text");
                }
                JSXChild::Element(element) => {
                    let element_span = element.span;
                    let element = self.take(element);
                    let expression = self.lower_element(element);
                    statements.push(self.statement(element_span, expression));
                }
                JSXChild::ExpressionContainer(container) => {
                    let container = self.take(container);
                    let container_span = container.span;
                    if !container.expression.is_expression() {
                        continue;
                    }
                    let mut expression = container.expression.into_expression();
                    if contains_jsx(&expression) && !matches!(expression, Expression::JSXElement(_))
                    {
                        self.error(
                            container_span,
                            "JSX control flow must use Show, For, Index, or Switch",
                        );
                        continue;
                    }
                    if matches!(expression, Expression::JSXElement(_)) {
                        self.visit_expression(&mut expression);
                        statements.push(self.statement(container_span, expression));
                        continue;
                    }
                    self.visit_expression(&mut expression);
                    let span = expression.span();
                    let text = Expression::new_call_expression(
                        span,
                        self.ident("String"),
                        None,
                        {
                            let mut arguments = self.ast_vec();
                            arguments.push(Argument::from(expression));
                            arguments
                        },
                        false,
                        &self.builder,
                    );
                    let content = self.arrow_expr(span, text);
                    let props = self.object(container_span, Vec::new());
                    let name = self.string_of("Text");
                    let call = self.call(RT_PRIMITIVE, vec![name, props, content], span);
                    statements.push(self.statement(container_span, call));
                }
                JSXChild::Fragment(fragment) => {
                    self.error(fragment.span, "JSX fragments are not supported");
                }
                JSXChild::Spread(spread) => {
                    self.error(spread.span, "JSX spread children are not supported");
                }
            }
        }
        self.arrow_block(span, statements)
    }

    /// Text content: a string for static text, a thunk when any part is dynamic.
    fn text_content(&mut self, span: Span, children: ArenaVec<'a, JSXChild<'a>>) -> Expression<'a> {
        let mut parts: Vec<TextPart<'a>> = Vec::new();
        for child in children {
            match child {
                JSXChild::Text(text) if text.value.trim().is_empty() => {}
                JSXChild::Text(text) => parts.push(TextPart::Text(text.value)),
                JSXChild::ExpressionContainer(container) => {
                    let container = self.take(container);
                    if !container.expression.is_expression() {
                        continue;
                    }
                    let mut expression = container.expression.into_expression();
                    self.visit_expression(&mut expression);
                    parts.push(TextPart::Expression(expression));
                }
                JSXChild::Element(element) => {
                    self.error(element.span, "Text children must be text or expressions");
                }
                JSXChild::Fragment(fragment) => {
                    self.error(fragment.span, "JSX fragments are not supported");
                }
                JSXChild::Spread(spread) => {
                    self.error(spread.span, "JSX spread children are not supported");
                }
            }
        }
        if parts.is_empty() {
            return self.string_of("");
        }
        let dynamic = parts
            .iter()
            .any(|part| matches!(part, TextPart::Expression(_)));
        if !dynamic {
            let mut text = String::new();
            for part in parts {
                if let TextPart::Text(value) = part {
                    text.push_str(value.as_str());
                }
            }
            return self.string_of(&text);
        }
        let mut iter = parts.into_iter();
        let first = iter.next().expect("parts is not empty");
        let mut expression = self.text_part(first);
        for part in iter {
            let right = self.text_part(part);
            expression = Expression::new_binary_expression(
                span,
                expression,
                BinaryOperator::Addition,
                right,
                &self.builder,
            );
        }
        self.arrow_expr(span, expression)
    }

    fn text_part(&mut self, part: TextPart<'a>) -> Expression<'a> {
        match part {
            TextPart::Text(value) => {
                let value = self.allocator.alloc_str(value.as_str());
                self.string(value, SPAN)
            }
            TextPart::Expression(mut expression) => {
                self.visit_expression(&mut expression);
                expression
            }
        }
    }
}

enum TextPart<'a> {
    Text(Str<'a>),
    Expression(Expression<'a>),
}

fn control_names(tag: &str) -> &'static [&'static str] {
    match tag {
        "Show" => &["when", "fallback"],
        "For" => &["each", "by"],
        "Index" => &["each"],
        "Switch" => &["fallback"],
        _ => &[],
    }
}

fn has_children(children: &ArenaVec<'_, JSXChild<'_>>) -> bool {
    children.iter().any(|child| match child {
        JSXChild::Text(text) => !text.value.trim().is_empty(),
        _ => true,
    })
}

fn contains_jsx(expression: &Expression<'_>) -> bool {
    struct Scan {
        found: bool,
    }
    impl<'a> Visit<'a> for Scan {
        fn visit_jsx_element(&mut self, _it: &JSXElement<'a>) {
            self.found = true;
        }
        fn visit_jsx_fragment(&mut self, _it: &JSXFragment<'a>) {
            self.found = true;
        }
    }
    let mut scan = Scan { found: false };
    scan.visit_expression(expression);
    scan.found
}

struct SignalScan<'s> {
    signals: &'s HashSet<String>,
    found: bool,
}

impl<'a, 's> Visit<'a> for SignalScan<'s> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if self.signals.contains(it.name.as_str()) {
            self.found = true;
        }
    }

    // A signal read inside a nested function is a callback, not a binding.
    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}

    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {}

    fn visit_jsx_element(&mut self, _it: &JSXElement<'a>) {}
}

impl<'a> Lower<'a> {
    fn signal_bindings(&self, statements: &ArenaVec<'a, Statement<'a>>) -> HashSet<String> {
        let mut bindings = HashSet::new();
        for statement in statements {
            let Statement::VariableDeclaration(declaration) = statement else {
                continue;
            };
            if declaration.kind != VariableDeclarationKind::Const {
                continue;
            }
            for declarator in &declaration.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    continue;
                };
                let Some(init) = &declarator.init else {
                    continue;
                };
                if is_signal_call(init) {
                    bindings.insert(id.name.as_str().to_string());
                }
            }
        }
        bindings
    }
}

fn is_signal_call(expression: &Expression<'_>) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    matches!(callee.name.as_str(), "signal" | "computed")
}

impl<'a> VisitMut<'a> for Lower<'a> {
    fn visit_program(&mut self, program: &mut Program<'a>) {
        // Exported signals are shared across modules and bind by value.
        let mut bindings = HashSet::new();
        for statement in &program.body {
            let Statement::VariableDeclaration(declaration) = statement else {
                continue;
            };
            if declaration.kind != VariableDeclarationKind::Const {
                continue;
            }
            for declarator in &declaration.declarations {
                let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                    continue;
                };
                if declarator.init.as_ref().is_some_and(is_signal_call) {
                    bindings.insert(id.name.as_str().to_string());
                }
            }
        }
        self.scopes.push(bindings);
        for statement in &mut program.body {
            self.visit_statement(statement);
        }
        self.scopes.pop();
    }

    fn visit_function(&mut self, function: &mut Function<'a>, flags: ScopeFlags) {
        let bindings = function
            .body
            .as_ref()
            .map(|body| self.signal_bindings(&body.statements))
            .unwrap_or_default();
        self.push_scope(bindings);
        oxc::ast_visit::walk_mut::walk_function(self, function, flags);
        self.scopes.pop();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &mut ArrowFunctionExpression<'a>) {
        let bindings = if let ArrowFunctionBody::FunctionBody(body) = &arrow.body {
            self.signal_bindings(&body.statements)
        } else {
            HashSet::new()
        };
        self.push_scope(bindings);
        oxc::ast_visit::walk_mut::walk_arrow_function_expression(self, arrow);
        self.scopes.pop();
    }

    fn visit_expression(&mut self, expression: &mut Expression<'a>) {
        if matches!(
            expression,
            Expression::JSXElement(_) | Expression::JSXFragment(_)
        ) {
            let span = expression.span();
            let taken = expression.take_in(&self.builder);
            let lowered = match taken {
                Expression::JSXElement(element) => {
                    let element = self.take(element);
                    self.lower_element(element)
                }
                Expression::JSXFragment(_) => {
                    self.error(span, "JSX fragments are not supported");
                    self.null(span)
                }
                _ => unreachable!("guarded above"),
            };
            *expression = lowered;
            return;
        }
        oxc::ast_visit::walk_mut::walk_expression(self, expression);
    }
}

impl<'a> Lower<'a> {
    /// Wrap top-level components, signals, and the mount call for `bubble dev`.
    fn hot_transform(&mut self, program: &mut Program<'a>, module_key: &str) {
        program.body.insert(0, hot_import(&self.builder));

        // Function declarations get a stable proxy registered right after them.
        let mut registrations = Vec::new();
        for (index, statement) in program.body.iter().enumerate() {
            let Statement::FunctionDeclaration(function) = statement else {
                continue;
            };
            let Some(id) = &function.id else {
                continue;
            };
            if is_component_name(id.name.as_str()) {
                let key = format!("{module_key}:{}", id.name.as_str());
                registrations.push((
                    index,
                    self.hot_register_function(id.span, id.name.as_str(), &key),
                ));
            }
        }

        for statement in &mut program.body {
            match statement {
                Statement::VariableDeclaration(declaration)
                    if declaration.kind == VariableDeclarationKind::Const =>
                {
                    self.hot_bindings(declaration, module_key);
                }
                Statement::ExpressionStatement(statement) => {
                    self.hot_mount(&mut statement.expression, module_key);
                }
                _ => {}
            }
        }

        for (index, registration) in registrations.into_iter().rev() {
            program.body.insert(index + 1, registration);
        }
    }

    fn hot_bindings(&self, declaration: &mut VariableDeclaration<'a>, module_key: &str) {
        for declarator in &mut declaration.declarations {
            let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };
            let Some(init) = declarator.init.as_mut() else {
                continue;
            };
            let name = id.name.as_str().to_string();
            let key = format!("{module_key}:{name}");
            if is_signal_call(init) {
                let inner = std::mem::replace(init, self.null(SPAN));
                let span = inner.span();
                *init = self.call(
                    HOT_KEEP,
                    vec![self.string_of(&key), self.arrow_expr(span, inner)],
                    span,
                );
            } else if is_component_name(&name)
                && matches!(
                    init,
                    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                )
            {
                let inner = std::mem::replace(init, self.null(SPAN));
                let span = inner.span();
                *init = self.call(HOT_COMPONENT, vec![self.string_of(&key), inner], span);
            }
        }
    }

    fn hot_mount(&self, expression: &mut Expression<'a>, module_key: &str) {
        let Expression::CallExpression(call) = expression else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "mount" {
            return;
        }
        let span = call.span;
        let taken = std::mem::replace(expression, self.null(span));
        let Expression::CallExpression(mut call) = taken else {
            return;
        };
        let mut arguments = call.arguments.take_in(&self.builder);
        if arguments.len() != 1 {
            return;
        }
        let argument = arguments.remove(0);
        let Ok(component) = Expression::try_from(argument) else {
            return;
        };
        *expression = self.call(HOT_MOUNT, vec![self.string_of(module_key), component], span);
    }

    fn hot_register_function(&self, span: Span, name: &'a str, key: &str) -> Statement<'a> {
        let target = AssignmentTarget::AssignmentTargetIdentifier(ArenaBox::new_in(
            IdentifierReference::new(span, Ident::from(name), &self.builder),
            &self.builder,
        ));
        let call = self.call(
            HOT_COMPONENT,
            vec![self.string_of(key), self.ident(name)],
            span,
        );
        let assignment = Expression::new_assignment_expression(
            span,
            AssignmentOperator::Assign,
            target,
            call,
            &self.builder,
        );
        self.statement(span, assignment)
    }
}

fn hot_import<'a>(builder: &AstBuilder<'a>) -> Statement<'a> {
    const BINDINGS: &[(&str, &str)] = &[
        ("component", HOT_COMPONENT),
        ("keep", HOT_KEEP),
        ("mount", HOT_MOUNT),
    ];
    let mut specifiers = ArenaVec::new_in(builder);
    for &(imported, local) in BINDINGS {
        let specifier = ImportSpecifier::new(
            SPAN,
            ModuleExportName::IdentifierName(IdentifierName::new(
                SPAN,
                Ident::from(imported),
                builder,
            )),
            BindingIdentifier::new(SPAN, Ident::from(local), builder),
            ImportOrExportKind::Value,
            builder,
        );
        specifiers.push(ImportDeclarationSpecifier::ImportSpecifier(
            ArenaBox::new_in(specifier, builder),
        ));
    }
    Statement::new_import_declaration(
        SPAN,
        Some(specifiers),
        StringLiteral::new(SPAN, Str::from(HOT_MODULE), None, builder),
        None,
        None,
        ImportOrExportKind::Value,
        builder,
    )
}

fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

// ---------------------------------------------------------------------------
// Import pruning
// ---------------------------------------------------------------------------

fn prune_shell_imports(program: &mut Program<'_>) {
    let mut collector = ReferenceCollector::default();
    collector.visit_program(program);
    let used = collector.names;

    let mut index = 0;
    while index < program.body.len() {
        let drop = match &mut program.body[index] {
            Statement::ImportDeclaration(declaration) => {
                let source = declaration.source.value.as_str();
                let is_shell = source == Some(SHELL_MODULE) || source == Some(RT_MODULE);
                match declaration.specifiers.as_mut() {
                    Some(specifiers) if is_shell => {
                        specifiers.retain(|specifier| {
                            let local = match specifier {
                                ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                                    &specifier.local
                                }
                                ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                                    &specifier.local
                                }
                                ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                                    &specifier.local
                                }
                            };
                            used.contains(local.name.as_str())
                        });
                        specifiers.is_empty()
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if drop {
            program.body.remove(index);
        } else {
            index += 1;
        }
    }
}

#[derive(Default)]
struct ReferenceCollector {
    names: HashSet<String>,
}

impl<'a> Visit<'a> for ReferenceCollector {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.names.insert(it.name.as_str().to_string());
    }
}
