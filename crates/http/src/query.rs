//! Parse `page`, `pageSize`, `sort` query params with v1 rules.

use rustapi_core::{ContentType, Error, ValidationErrors};
use rustapi_sql::{Sort, SortDir};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct ListParams {
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(rename = "pageSize", default)]
    pub page_size: Option<u32>,
    #[serde(default)]
    pub sort: Option<String>,
}

#[derive(Debug)]
pub struct ListOpts {
    pub page: u32,
    pub page_size: u32,
    pub sort: Sort,
}

pub fn parse_list(ct: &ContentType, params: ListParams, page_size_max: u32) -> Result<ListOpts, Error> {
    let page = params.page.unwrap_or(1).max(1);
    let mut page_size = params.page_size.unwrap_or(25);
    if page_size == 0 {
        page_size = 25;
    }
    if page_size > page_size_max {
        page_size = page_size_max;
    }

    let sort = match params.sort {
        None => Sort::default_created_at(),
        Some(s) => parse_sort(&s, ct)?,
    };

    Ok(ListOpts { page, page_size, sort })
}

fn parse_sort(s: &str, ct: &ContentType) -> Result<Sort, Error> {
    let (col, dir_str) = match s.split_once(':') {
        Some((c, d)) => (c, d),
        None => (s, "asc"),
    };
    let dir = SortDir::parse(dir_str).ok_or_else(|| {
        Error::Validation(ValidationErrors::single("sort direction must be asc or desc"))
    })?;
    if !is_sortable(col, ct) {
        return Err(Error::Validation(ValidationErrors::single(format!(
            "unknown sort field `{col}`"
        ))));
    }
    Ok(Sort {
        column: col.to_string(),
        dir,
    })
}

fn is_sortable(col: &str, ct: &ContentType) -> bool {
    if rustapi_core::is_system_column(col) {
        return true;
    }
    ct.fields.iter().any(|f| f.name == col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::{Field, FieldKind};
    use serde_json::json;
    use uuid::Uuid;

    fn ct() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "title".into(),
                kind: FieldKind::String,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn defaults() {
        let opts = parse_list(&ct(), ListParams::default(), 100).unwrap();
        assert_eq!(opts.page, 1);
        assert_eq!(opts.page_size, 25);
        assert_eq!(opts.sort.column, "created_at");
    }

    #[test]
    fn caps_page_size() {
        let opts = parse_list(
            &ct(),
            ListParams { page: None, page_size: Some(9999), sort: None },
            50,
        ).unwrap();
        assert_eq!(opts.page_size, 50);
    }

    #[test]
    fn sort_user_field() {
        let opts = parse_list(
            &ct(),
            ListParams { page: None, page_size: None, sort: Some("title:desc".into()) },
            100,
        ).unwrap();
        assert_eq!(opts.sort.column, "title");
        assert_eq!(opts.sort.dir, SortDir::Desc);
    }

    #[test]
    fn sort_unknown_field_rejected() {
        let r = parse_list(
            &ct(),
            ListParams { page: None, page_size: None, sort: Some("nope".into()) },
            100,
        );
        assert!(matches!(r, Err(Error::Validation(_))));
    }

    #[test]
    fn sort_bad_dir_rejected() {
        let r = parse_list(
            &ct(),
            ListParams { page: None, page_size: None, sort: Some("title:sideways".into()) },
            100,
        );
        assert!(matches!(r, Err(Error::Validation(_))));
    }
}
