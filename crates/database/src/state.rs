use crate::database_description::DatabaseDescription;

// pub mod data_source;
// pub mod generic_database;
//#[cfg(feature = "rocksdb")]
// pub mod historical_rocksdb;
// pub mod in_memory;
pub mod iterable_key_value_view;
pub mod key_value_view;
//#[cfg(feature = "rocksdb")]
pub mod rocks_db;
//#[cfg(feature = "rocksdb")]
pub mod rocks_db_key_iterator;

pub type ColumnType<Description> = <Description as DatabaseDescription>::Column;
pub type HeightType<Description> = <Description as DatabaseDescription>::Height;
