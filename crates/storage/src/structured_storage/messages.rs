//! The module contains implementations and tests for the messages tables.

use crate::{
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structure::plain::Plain,
    structured_storage::TableWithStructure,
    tables::{
        Messages,
        SpentMessages,
    },
};

impl TableWithStructure for Messages {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::Messages
    }
}

impl TableWithStructure for SpentMessages {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::SpentMessages
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

    crate::basic_storage_tests!(
        SpentMessages,
        <SpentMessages as crate::Mappable>::Key::default(),
        <SpentMessages as crate::Mappable>::Value::default()
    );
}
