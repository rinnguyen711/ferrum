#![forbid(unsafe_code)]

pub mod ddl;
pub mod dml;
pub mod filter;
pub mod ident;
pub mod sort;

pub use ddl::{add_column, create_table, drop_column, drop_table, DdlError};
pub use dml::{
    count, delete, insert, render_where, select_by_id, select_list, update,
    DmlError, SqlAndBinds,
};
pub use filter::{op_allows_kind, Condition, Filter, FilterValue, Op};
pub use ident::{quote_ident, table_name, IdentError};
pub use sort::{Sort, SortDir};
