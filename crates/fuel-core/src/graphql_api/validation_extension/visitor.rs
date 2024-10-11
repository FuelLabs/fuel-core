//! The code below is copy from `<https://github.com/async-graphql/async-graphql/blob/master/src/validation/visitor.rs>`.

#![allow(dead_code)]
#![allow(clippy::arithmetic_side_effects)]

use async_graphql::{
    parser::types::{
        Directive,
        ExecutableDocument,
        Field,
        FragmentDefinition,
        FragmentSpread,
        InlineFragment,
        OperationDefinition,
        OperationType,
        Selection,
        SelectionSet,
        TypeCondition,
        VariableDefinition,
    },
    registry::{
        self,
        MetaType,
        MetaTypeName,
    },
    InputType,
    Name,
    Pos,
    Positioned,
    ServerError,
    ServerResult,
    Variables,
};
use async_graphql_value::Value;
use std::{
    collections::HashMap,
    fmt::{
        self,
        Display,
        Formatter,
    },
};

#[doc(hidden)]
pub struct VisitorContext<'a> {
    pub(crate) registry: &'a registry::Registry,
    pub(crate) variables: Option<&'a Variables>,
    pub(crate) errors: Vec<RuleError>,
    pub(crate) type_stack: Vec<Option<&'a registry::MetaType>>,
    input_type: Vec<Option<MetaTypeName<'a>>>,
    fragments: &'a HashMap<Name, Positioned<FragmentDefinition>>,
}

impl<'a> VisitorContext<'a> {
    pub(crate) fn new(
        registry: &'a registry::Registry,
        doc: &'a ExecutableDocument,
        variables: Option<&'a Variables>,
    ) -> Self {
        Self {
            registry,
            variables,
            errors: Default::default(),
            type_stack: Default::default(),
            input_type: Default::default(),
            fragments: &doc.fragments,
        }
    }

    pub(crate) fn report_error<T: Into<String>>(&mut self, locations: Vec<Pos>, msg: T) {
        self.errors.push(RuleError::new(locations, msg));
    }

    pub(crate) fn append_errors(&mut self, errors: Vec<RuleError>) {
        self.errors.extend(errors);
    }

    pub(crate) fn with_type<F: FnMut(&mut VisitorContext<'a>)>(
        &mut self,
        ty: Option<&'a registry::MetaType>,
        mut f: F,
    ) {
        self.type_stack.push(ty);
        f(self);
        self.type_stack.pop();
    }

    pub(crate) fn with_input_type<F: FnMut(&mut VisitorContext<'a>)>(
        &mut self,
        ty: Option<MetaTypeName<'a>>,
        mut f: F,
    ) {
        self.input_type.push(ty);
        f(self);
        self.input_type.pop();
    }

    pub(crate) fn parent_type(&self) -> Option<&'a registry::MetaType> {
        if self.type_stack.len() >= 2 {
            self.type_stack
                .get(self.type_stack.len() - 2)
                .copied()
                .flatten()
        } else {
            None
        }
    }

    pub(crate) fn current_type(&self) -> Option<&'a registry::MetaType> {
        self.type_stack.last().copied().flatten()
    }

    pub(crate) fn is_known_fragment(&self, name: &str) -> bool {
        self.fragments.contains_key(name)
    }

    pub(crate) fn fragment(
        &self,
        name: &str,
    ) -> Option<&'a Positioned<FragmentDefinition>> {
        self.fragments.get(name)
    }

    #[doc(hidden)]
    pub fn param_value<T: InputType>(
        &self,
        variable_definitions: &[Positioned<VariableDefinition>],
        field: &Field,
        name: &str,
        default: Option<fn() -> T>,
    ) -> ServerResult<T> {
        let value = field.get_argument(name).cloned();

        if value.is_none() {
            if let Some(default) = default {
                return Ok(default());
            }
        }

        let (pos, value) = match value {
            Some(value) => {
                let pos = value.pos;
                (
                    pos,
                    Some(value.node.into_const_with(|name| {
                        variable_definitions
                            .iter()
                            .find(|def| def.node.name.node == name)
                            .and_then(|def| {
                                if let Some(variables) = self.variables {
                                    variables
                                        .get(&def.node.name.node)
                                        .or_else(|| def.node.default_value())
                                } else {
                                    None
                                }
                            })
                            .cloned()
                            .ok_or_else(|| {
                                ServerError::new(
                                    format!("Variable {} is not defined.", name),
                                    Some(pos),
                                )
                            })
                    })?),
                )
            }
            None => (Pos::default(), None),
        };

        T::parse(value).map_err(|e| e.into_server_error(pos))
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum VisitMode {
    Normal,
    Inline,
}

pub(crate) trait Visitor<'a> {
    fn mode(&self) -> VisitMode {
        VisitMode::Normal
    }

    fn enter_document(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _doc: &'a ExecutableDocument,
    ) {
    }
    fn exit_document(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _doc: &'a ExecutableDocument,
    ) {
    }

    fn enter_operation_definition(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _name: Option<&'a Name>,
        _operation_definition: &'a Positioned<OperationDefinition>,
    ) {
    }
    fn exit_operation_definition(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _name: Option<&'a Name>,
        _operation_definition: &'a Positioned<OperationDefinition>,
    ) {
    }

    fn enter_fragment_definition(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _name: &'a Name,
        _fragment_definition: &'a Positioned<FragmentDefinition>,
    ) {
    }
    fn exit_fragment_definition(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _name: &'a Name,
        _fragment_definition: &'a Positioned<FragmentDefinition>,
    ) {
    }

    fn enter_variable_definition(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _variable_definition: &'a Positioned<VariableDefinition>,
    ) {
    }
    fn exit_variable_definition(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _variable_definition: &'a Positioned<VariableDefinition>,
    ) {
    }

    fn enter_directive(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _directive: &'a Positioned<Directive>,
    ) {
    }
    fn exit_directive(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _directive: &'a Positioned<Directive>,
    ) {
    }

    fn enter_argument(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _name: &'a Positioned<Name>,
        _value: &'a Positioned<Value>,
    ) {
    }
    fn exit_argument(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _name: &'a Positioned<Name>,
        _value: &'a Positioned<Value>,
    ) {
    }

    fn enter_selection_set(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _selection_set: &'a Positioned<SelectionSet>,
    ) {
    }
    fn exit_selection_set(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _selection_set: &'a Positioned<SelectionSet>,
    ) {
    }

    fn enter_selection(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _selection: &'a Positioned<Selection>,
    ) {
    }
    fn exit_selection(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _selection: &'a Positioned<Selection>,
    ) {
    }

    fn enter_field(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _field: &'a Positioned<Field>,
    ) {
    }
    fn exit_field(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _field: &'a Positioned<Field>,
    ) {
    }

    fn enter_fragment_spread(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _fragment_spread: &'a Positioned<FragmentSpread>,
    ) {
    }
    fn exit_fragment_spread(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _fragment_spread: &'a Positioned<FragmentSpread>,
    ) {
    }

    fn enter_inline_fragment(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _inline_fragment: &'a Positioned<InlineFragment>,
    ) {
    }
    fn exit_inline_fragment(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _inline_fragment: &'a Positioned<InlineFragment>,
    ) {
    }

    fn enter_input_value(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _pos: Pos,
        _expected_type: &Option<MetaTypeName<'a>>,
        _value: &'a Value,
    ) {
    }
    fn exit_input_value(
        &mut self,
        _ctx: &mut VisitorContext<'a>,
        _pos: Pos,
        _expected_type: &Option<MetaTypeName<'a>>,
        _value: &Value,
    ) {
    }
}

pub(crate) struct VisitorNil;

impl VisitorNil {
    pub(crate) fn with<V>(self, visitor: V) -> VisitorCons<V, Self> {
        VisitorCons(visitor, self)
    }
}

pub(crate) struct VisitorCons<A, B>(A, B);

impl<A, B> VisitorCons<A, B> {
    pub(crate) const fn with<V>(self, visitor: V) -> VisitorCons<V, Self> {
        VisitorCons(visitor, self)
    }
}

impl<'a> Visitor<'a> for VisitorNil {}

impl<'a, A, B> Visitor<'a> for VisitorCons<A, B>
where
    A: Visitor<'a> + 'a,
    B: Visitor<'a> + 'a,
{
    fn mode(&self) -> VisitMode {
        self.0.mode()
    }

    fn enter_document(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        doc: &'a ExecutableDocument,
    ) {
        self.0.enter_document(ctx, doc);
        self.1.enter_document(ctx, doc);
    }

    fn exit_document(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        doc: &'a ExecutableDocument,
    ) {
        self.0.exit_document(ctx, doc);
        self.1.exit_document(ctx, doc);
    }

    fn enter_operation_definition(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        name: Option<&'a Name>,
        operation_definition: &'a Positioned<OperationDefinition>,
    ) {
        self.0
            .enter_operation_definition(ctx, name, operation_definition);
        self.1
            .enter_operation_definition(ctx, name, operation_definition);
    }

    fn exit_operation_definition(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        name: Option<&'a Name>,
        operation_definition: &'a Positioned<OperationDefinition>,
    ) {
        self.0
            .exit_operation_definition(ctx, name, operation_definition);
        self.1
            .exit_operation_definition(ctx, name, operation_definition);
    }

    fn enter_fragment_definition(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        name: &'a Name,
        fragment_definition: &'a Positioned<FragmentDefinition>,
    ) {
        self.0
            .enter_fragment_definition(ctx, name, fragment_definition);
        self.1
            .enter_fragment_definition(ctx, name, fragment_definition);
    }

    fn exit_fragment_definition(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        name: &'a Name,
        fragment_definition: &'a Positioned<FragmentDefinition>,
    ) {
        self.0
            .exit_fragment_definition(ctx, name, fragment_definition);
        self.1
            .exit_fragment_definition(ctx, name, fragment_definition);
    }

    fn enter_variable_definition(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        variable_definition: &'a Positioned<VariableDefinition>,
    ) {
        self.0.enter_variable_definition(ctx, variable_definition);
        self.1.enter_variable_definition(ctx, variable_definition);
    }

    fn exit_variable_definition(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        variable_definition: &'a Positioned<VariableDefinition>,
    ) {
        self.0.exit_variable_definition(ctx, variable_definition);
        self.1.exit_variable_definition(ctx, variable_definition);
    }

    fn enter_directive(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        directive: &'a Positioned<Directive>,
    ) {
        self.0.enter_directive(ctx, directive);
        self.1.enter_directive(ctx, directive);
    }

    fn exit_directive(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        directive: &'a Positioned<Directive>,
    ) {
        self.0.exit_directive(ctx, directive);
        self.1.exit_directive(ctx, directive);
    }

    fn enter_argument(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        name: &'a Positioned<Name>,
        value: &'a Positioned<Value>,
    ) {
        self.0.enter_argument(ctx, name, value);
        self.1.enter_argument(ctx, name, value);
    }

    fn exit_argument(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        name: &'a Positioned<Name>,
        value: &'a Positioned<Value>,
    ) {
        self.0.exit_argument(ctx, name, value);
        self.1.exit_argument(ctx, name, value);
    }

    fn enter_selection_set(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        selection_set: &'a Positioned<SelectionSet>,
    ) {
        self.0.enter_selection_set(ctx, selection_set);
        self.1.enter_selection_set(ctx, selection_set);
    }

    fn exit_selection_set(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        selection_set: &'a Positioned<SelectionSet>,
    ) {
        self.0.exit_selection_set(ctx, selection_set);
        self.1.exit_selection_set(ctx, selection_set);
    }

    fn enter_selection(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        selection: &'a Positioned<Selection>,
    ) {
        self.0.enter_selection(ctx, selection);
        self.1.enter_selection(ctx, selection);
    }

    fn exit_selection(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        selection: &'a Positioned<Selection>,
    ) {
        self.0.exit_selection(ctx, selection);
        self.1.exit_selection(ctx, selection);
    }

    fn enter_field(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        field: &'a Positioned<Field>,
    ) {
        self.0.enter_field(ctx, field);
        self.1.enter_field(ctx, field);
    }

    fn exit_field(&mut self, ctx: &mut VisitorContext<'a>, field: &'a Positioned<Field>) {
        self.0.exit_field(ctx, field);
        self.1.exit_field(ctx, field);
    }

    fn enter_fragment_spread(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        fragment_spread: &'a Positioned<FragmentSpread>,
    ) {
        self.0.enter_fragment_spread(ctx, fragment_spread);
        self.1.enter_fragment_spread(ctx, fragment_spread);
    }

    fn exit_fragment_spread(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        fragment_spread: &'a Positioned<FragmentSpread>,
    ) {
        self.0.exit_fragment_spread(ctx, fragment_spread);
        self.1.exit_fragment_spread(ctx, fragment_spread);
    }

    fn enter_inline_fragment(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        inline_fragment: &'a Positioned<InlineFragment>,
    ) {
        self.0.enter_inline_fragment(ctx, inline_fragment);
        self.1.enter_inline_fragment(ctx, inline_fragment);
    }

    fn exit_inline_fragment(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        inline_fragment: &'a Positioned<InlineFragment>,
    ) {
        self.0.exit_inline_fragment(ctx, inline_fragment);
        self.1.exit_inline_fragment(ctx, inline_fragment);
    }
}

pub(crate) fn visit<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    doc: &'a ExecutableDocument,
) {
    v.enter_document(ctx, doc);

    for (name, fragment) in &doc.fragments {
        ctx.with_type(
            ctx.registry
                .types
                .get(fragment.node.type_condition.node.on.node.as_str()),
            |ctx| visit_fragment_definition(v, ctx, name, fragment),
        )
    }

    for (name, operation) in doc.operations.iter() {
        visit_operation_definition(v, ctx, name, operation);
    }

    v.exit_document(ctx, doc);
}

fn visit_operation_definition<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    name: Option<&'a Name>,
    operation: &'a Positioned<OperationDefinition>,
) {
    v.enter_operation_definition(ctx, name, operation);
    let root_name = match &operation.node.ty {
        OperationType::Query => Some(&*ctx.registry.query_type),
        OperationType::Mutation => ctx.registry.mutation_type.as_deref(),
        OperationType::Subscription => ctx.registry.subscription_type.as_deref(),
    };
    if let Some(root_name) = root_name {
        ctx.with_type(Some(&ctx.registry.types[root_name]), |ctx| {
            visit_variable_definitions(v, ctx, &operation.node.variable_definitions);
            visit_directives(v, ctx, &operation.node.directives);
            visit_selection_set(v, ctx, &operation.node.selection_set);
        });
    } else {
        ctx.report_error(
            vec![operation.pos],
            // The only one with an irregular plural, "query", is always present
            format!("Schema is not configured for {}s.", operation.node.ty),
        );
    }
    v.exit_operation_definition(ctx, name, operation);
}

fn visit_selection_set<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    selection_set: &'a Positioned<SelectionSet>,
) {
    if !selection_set.node.items.is_empty() {
        v.enter_selection_set(ctx, selection_set);
        for selection in &selection_set.node.items {
            visit_selection(v, ctx, selection);
        }
        v.exit_selection_set(ctx, selection_set);
    }
}

fn visit_selection<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    selection: &'a Positioned<Selection>,
) {
    v.enter_selection(ctx, selection);
    match &selection.node {
        Selection::Field(field) => {
            if field.node.name.node != "__typename" {
                ctx.with_type(
                    ctx.current_type()
                        .and_then(|ty| ty.field_by_name(&field.node.name.node))
                        .and_then(|schema_field| {
                            ctx.registry.concrete_type_by_name(&schema_field.ty)
                        }),
                    |ctx| {
                        visit_field(v, ctx, field);
                    },
                );
            } else if ctx.current_type().map(|ty| match ty {
                MetaType::Object {
                    is_subscription, ..
                } => *is_subscription,
                _ => false,
            }) == Some(true)
            {
                ctx.report_error(
                    vec![field.pos],
                    "Unknown field \"__typename\" on type \"Subscription\".",
                );
            }
        }
        Selection::FragmentSpread(fragment_spread) => {
            visit_fragment_spread(v, ctx, fragment_spread)
        }
        Selection::InlineFragment(inline_fragment) => {
            if let Some(TypeCondition { on: name }) = &inline_fragment
                .node
                .type_condition
                .as_ref()
                .map(|c| &c.node)
            {
                ctx.with_type(ctx.registry.types.get(name.node.as_str()), |ctx| {
                    visit_inline_fragment(v, ctx, inline_fragment)
                });
            } else {
                visit_inline_fragment(v, ctx, inline_fragment)
            }
        }
    }
    v.exit_selection(ctx, selection);
}

fn visit_field<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    field: &'a Positioned<Field>,
) {
    v.enter_field(ctx, field);

    for (name, value) in &field.node.arguments {
        v.enter_argument(ctx, name, value);
        let expected_ty = ctx
            .parent_type()
            .and_then(|ty| ty.field_by_name(&field.node.name.node))
            .and_then(|schema_field| schema_field.args.get(&*name.node))
            .map(|input_ty| MetaTypeName::create(&input_ty.ty));
        ctx.with_input_type(expected_ty, |ctx| {
            visit_input_value(v, ctx, field.pos, expected_ty, &value.node)
        });
        v.exit_argument(ctx, name, value);
    }

    visit_directives(v, ctx, &field.node.directives);
    visit_selection_set(v, ctx, &field.node.selection_set);
    v.exit_field(ctx, field);
}

fn visit_input_value<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    pos: Pos,
    expected_ty: Option<MetaTypeName<'a>>,
    value: &'a Value,
) {
    v.enter_input_value(ctx, pos, &expected_ty, value);

    match value {
        Value::List(values) => {
            if let Some(expected_ty) = expected_ty {
                let elem_ty = expected_ty.unwrap_non_null();
                if let MetaTypeName::List(expected_ty) = elem_ty {
                    values.iter().for_each(|value| {
                        visit_input_value(
                            v,
                            ctx,
                            pos,
                            Some(MetaTypeName::create(expected_ty)),
                            value,
                        )
                    });
                }
            }
        }
        Value::Object(values) => {
            if let Some(expected_ty) = expected_ty {
                let expected_ty = expected_ty.unwrap_non_null();
                if let MetaTypeName::Named(expected_ty) = expected_ty {
                    if let Some(MetaType::InputObject { input_fields, .. }) = ctx
                        .registry
                        .types
                        .get(MetaTypeName::concrete_typename(expected_ty))
                    {
                        for (item_key, item_value) in values {
                            if let Some(input_value) = input_fields.get(item_key.as_str())
                            {
                                visit_input_value(
                                    v,
                                    ctx,
                                    pos,
                                    Some(MetaTypeName::create(&input_value.ty)),
                                    item_value,
                                );
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    v.exit_input_value(ctx, pos, &expected_ty, value);
}

fn visit_variable_definitions<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    variable_definitions: &'a [Positioned<VariableDefinition>],
) {
    for d in variable_definitions {
        v.enter_variable_definition(ctx, d);
        v.exit_variable_definition(ctx, d);
    }
}

fn visit_directives<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    directives: &'a [Positioned<Directive>],
) {
    for d in directives {
        v.enter_directive(ctx, d);

        let schema_directive = ctx.registry.directives.get(d.node.name.node.as_str());

        for (name, value) in &d.node.arguments {
            v.enter_argument(ctx, name, value);
            let expected_ty = schema_directive
                .and_then(|schema_directive| schema_directive.args.get(&*name.node))
                .map(|input_ty| MetaTypeName::create(&input_ty.ty));
            ctx.with_input_type(expected_ty, |ctx| {
                visit_input_value(v, ctx, d.pos, expected_ty, &value.node)
            });
            v.exit_argument(ctx, name, value);
        }

        v.exit_directive(ctx, d);
    }
}

fn visit_fragment_definition<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    name: &'a Name,
    fragment: &'a Positioned<FragmentDefinition>,
) {
    if v.mode() == VisitMode::Normal {
        v.enter_fragment_definition(ctx, name, fragment);
        visit_directives(v, ctx, &fragment.node.directives);
        visit_selection_set(v, ctx, &fragment.node.selection_set);
        v.exit_fragment_definition(ctx, name, fragment);
    }
}

fn visit_fragment_spread<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    fragment_spread: &'a Positioned<FragmentSpread>,
) {
    v.enter_fragment_spread(ctx, fragment_spread);
    visit_directives(v, ctx, &fragment_spread.node.directives);
    if v.mode() == VisitMode::Inline {
        if let Some(fragment) = ctx
            .fragments
            .get(fragment_spread.node.fragment_name.node.as_str())
        {
            visit_selection_set(v, ctx, &fragment.node.selection_set);
        }
    }
    v.exit_fragment_spread(ctx, fragment_spread);
}

fn visit_inline_fragment<'a, V: Visitor<'a>>(
    v: &mut V,
    ctx: &mut VisitorContext<'a>,
    inline_fragment: &'a Positioned<InlineFragment>,
) {
    v.enter_inline_fragment(ctx, inline_fragment);
    visit_directives(v, ctx, &inline_fragment.node.directives);
    visit_selection_set(v, ctx, &inline_fragment.node.selection_set);
    v.exit_inline_fragment(ctx, inline_fragment);
}

#[derive(Debug, PartialEq)]
pub(crate) struct RuleError {
    pub(crate) locations: Vec<Pos>,
    pub(crate) message: String,
}

impl RuleError {
    pub(crate) fn new(locations: Vec<Pos>, msg: impl Into<String>) -> Self {
        Self {
            locations,
            message: msg.into(),
        }
    }
}

impl Display for RuleError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (idx, loc) in self.locations.iter().enumerate() {
            if idx == 0 {
                write!(f, "[")?;
            } else {
                write!(f, ", ")?;
            }

            write!(f, "{}:{}", loc.line, loc.column)?;

            if idx == self.locations.len() - 1 {
                write!(f, "] ")?;
            }
        }

        write!(f, "{}", self.message)?;
        Ok(())
    }
}

impl From<RuleError> for ServerError {
    fn from(e: RuleError) -> Self {
        Self {
            message: e.message,
            source: None,
            locations: e.locations,
            path: Vec::new(),
            extensions: None,
        }
    }
}
