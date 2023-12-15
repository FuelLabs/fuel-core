use crate::prelude::q;

pub trait Visitor<E> {
    fn enter_field(&mut self, _: &q::Field) -> Result<(), E> {
        Ok(())
    }
    fn leave_field(&mut self, _: &mut q::Field) -> Result<(), E> {
        Ok(())
    }

    fn enter_query(&mut self, _: &q::Query) -> Result<(), E> {
        Ok(())
    }
    fn leave_query(&mut self, _: &mut q::Query) -> Result<(), E> {
        Ok(())
    }

    fn visit_fragment_spread(&mut self, _: &q::FragmentSpread) -> Result<(), E> {
        Ok(())
    }
}

pub fn visit<E, T>(visitor: &mut dyn Visitor<E>, doc: &mut q::Document) -> Result<(), E> {
    for def in &mut doc.definitions {
        match def {
            q::Definition::Operation(op) => match op {
                q::OperationDefinition::SelectionSet(set) => {
                    visit_selection_set(visitor, set)?;
                }
                q::OperationDefinition::Query(query) => {
                    visitor.enter_query(query)?;
                    visit_selection_set(visitor, &mut query.selection_set)?;
                    visitor.leave_query(query)?;
                }
                q::OperationDefinition::Mutation(_) => todo!(),
                q::OperationDefinition::Subscription(_) => todo!(),
            },
            q::Definition::Fragment(frag) => {}
        }
    }
    Ok(())
}

fn visit_selection_set<E>(
    visitor: &mut dyn Visitor<E>,
    set: &mut q::SelectionSet,
) -> Result<(), E> {
    for sel in &mut set.items {
        match sel {
            q::Selection::Field(field) => {
                visitor.enter_field(field)?;
                visit_selection_set(visitor, &mut field.selection_set)?;
                visitor.leave_field(field)?;
            }
            q::Selection::FragmentSpread(frag) => {
                visitor.visit_fragment_spread(frag)?;
            }
            q::Selection::InlineFragment(frag) => {}
        }
    }
    Ok(())
}
