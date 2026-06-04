#![forbid(unsafe_code)]

pub mod ddl;
pub mod dml;
pub mod filter;
pub mod ident;
pub mod sort;

pub use ddl::{
    add_column, alter_enum_values, create_join_table, create_media_join_table, create_table,
    drop_column, drop_join_table, drop_media_join_table, drop_table, DdlError,
};
pub use dml::{
    count, delete, delete_links, insert, insert_links, render_where,
    select_by_id, select_list, update, DmlError, SqlAndBinds,
};
pub use filter::{op_allows_kind, Condition, Filter, FilterValue, Op};
pub use ident::{join_table_name, quote_ident, table_name, IdentError};
pub use sort::{Sort, SortDir};
