use graph::prelude::q::*;

use graph::prelude::QueryExecutionError;

/// Returns the operation for the given name (or the only operation if no name is defined).
pub fn get_operation<'a>(
    document: &'a Document,
    name: Option<&str>,
) -> Result<&'a OperationDefinition, QueryExecutionError> {
    let operations = get_operations(document);

    match (name, operations.len()) {
        (None, 1) => Ok(operations[0]),
        (None, _) => Err(QueryExecutionError::OperationNameRequired),
        (Some(s), n) if n > 0 => operations
            .into_iter()
            .find(|op| match get_operation_name(op) {
                Some(n) => s == n,
                None => false,
            })
            .ok_or_else(|| QueryExecutionError::OperationNotFound(s.to_string())),
        _ => Err(QueryExecutionError::OperationNameRequired),
    }
}

/// Returns all operation definitions in the document.
pub fn get_operations(document: &Document) -> Vec<&OperationDefinition> {
    document
        .definitions
        .iter()
        .filter_map(|d| match d {
            Definition::Operation(op) => Some(op),
            _ => None,
        })
        .collect()
}

/// Returns the name of the given operation (if it has one).
pub fn get_operation_name(operation: &OperationDefinition) -> Option<&str> {
    match operation {
        OperationDefinition::Mutation(m) => m.name.as_deref(),
        OperationDefinition::Query(q) => q.name.as_deref(),
        OperationDefinition::SelectionSet(_) => None,
        OperationDefinition::Subscription(s) => s.name.as_deref(),
    }
}

/// Looks up a directive in a selection, if it is provided.
pub fn get_directive(selection: &Selection, name: String) -> Option<&Directive> {
    match selection {
        Selection::Field(field) => field
            .directives
            .iter()
            .find(|directive| directive.name == name),
        _ => None,
    }
}

/// Looks up the value of an argument in a vector of (name, value) tuples.
pub fn get_argument_value<'a>(arguments: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    arguments.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

/// Returns the variable definitions for an operation.
pub fn get_variable_definitions(
    operation: &OperationDefinition,
) -> Option<&Vec<VariableDefinition>> {
    match operation {
        OperationDefinition::Query(q) => Some(&q.variable_definitions),
        OperationDefinition::Subscription(s) => Some(&s.variable_definitions),
        OperationDefinition::Mutation(m) => Some(&m.variable_definitions),
        OperationDefinition::SelectionSet(_) => None,
    }
}
