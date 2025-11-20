//! The module contains implementations and tests for the messages tables.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::Messages,
};

impl TableWithBlueprint for Messages {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::Messages
    }
}

#[cfg(test)]
mod test {
    use super::*;

    crate::basic_storage_tests!(
        Messages,
        <Messages as crate::Mappable>::Key::default(),
        <Messages as crate::Mappable>::Value::default()
    );
}
