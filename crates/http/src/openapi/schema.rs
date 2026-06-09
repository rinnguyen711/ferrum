//! Maps the content-type field model to OpenAPI/JSON Schema fragments.

use rustapi_core::field::{Cardinality, Field, FieldKind};
use rustapi_core::ContentType;
use serde_json::{json, Value};

/// Build a JSON Schema fragment for a single field's value type.
pub fn field_to_schema(field: &Field) -> Value {
    let mut schema = match field.kind {
        FieldKind::String => {
            json!({ "type": "string", "maxLength": field.effective_max_length() })
        }
        FieldKind::Text => match field.max_length {
            Some(n) => json!({ "type": "string", "maxLength": n }),
            None => json!({ "type": "string" }),
        },
        FieldKind::Integer => json!({ "type": "integer", "format": "int64" }),
        FieldKind::Float => json!({ "type": "number", "format": "double" }),
        FieldKind::Boolean => json!({ "type": "boolean" }),
        FieldKind::Datetime => json!({ "type": "string", "format": "date-time" }),
        FieldKind::Uuid => json!({ "type": "string", "format": "uuid" }),
        FieldKind::Email => json!({ "type": "string", "format": "email" }),
        FieldKind::Url => json!({ "type": "string", "format": "uri" }),
        FieldKind::Slug => {
            json!({ "type": "string", "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$" })
        }
        FieldKind::Enum => {
            let values = field.enum_meta().map(|m| m.values).unwrap_or_default();
            if values.is_empty() {
                json!({ "type": "string" })
            } else {
                json!({ "type": "string", "enum": values })
            }
        }
        FieldKind::Json => json!({}),
        FieldKind::Relation => {
            let many = field
                .relation_meta()
                .map(|m| matches!(m.cardinality, Cardinality::ManyToMany))
                .unwrap_or(false);
            if many {
                json!({ "type": "array", "items": { "type": "string", "format": "uuid" } })
            } else {
                // Covers many_to_one and one_to_one (both single-FK).
                // None from relation_meta() also falls back to this single-UUID shape.
                json!({ "type": "string", "format": "uuid" })
            }
        }
        FieldKind::Media => {
            let multiple = field.media_meta().map(|m| m.multiple).unwrap_or(false);
            if multiple {
                json!({ "type": "array", "items": { "type": "string", "format": "uuid" } })
            } else {
                json!({ "type": "string", "format": "uuid" })
            }
        }
        // FieldKind is #[non_exhaustive]; stay permissive for future kinds.
        _ => json!({}),
    };
    if !field.default.is_null() {
        if let Value::Object(ref mut map) = schema {
            map.insert("default".into(), field.default.clone());
        }
    }
    schema
}

/// Returns (response_schema_name, request_schema_name) for a content type.
pub fn schema_names(ct_name: &str) -> (String, String) {
    let pascal = to_pascal(ct_name);
    (pascal.clone(), format!("{pascal}Input"))
}

fn to_pascal(name: &str) -> String {
    name.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                Some(first) => first.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

/// Build response (`T`) and request (`TInput`) component schemas for a type.
pub fn content_type_schemas(ct: &ContentType) -> Value {
    let (resp_name, req_name) = schema_names(&ct.name);

    let mut resp_props = serde_json::Map::new();
    resp_props.insert("id".into(), json!({ "type": "string", "format": "uuid" }));
    resp_props.insert("created_at".into(), json!({ "type": "string", "format": "date-time" }));
    resp_props.insert("updated_at".into(), json!({ "type": "string", "format": "date-time" }));

    let mut req_props = serde_json::Map::new();
    let mut req_required: Vec<String> = Vec::new();
    let mut resp_required: Vec<String> =
        vec!["id".into(), "created_at".into(), "updated_at".into()];

    for field in &ct.fields {
        let s = field_to_schema(field);
        resp_props.insert(field.name.clone(), s.clone());
        req_props.insert(field.name.clone(), s);
        if field.required {
            req_required.push(field.name.clone());
            resp_required.push(field.name.clone());
        }
    }

    json!({
        resp_name: {
            "type": "object",
            "properties": Value::Object(resp_props),
            "required": resp_required,
        },
        req_name: {
            "type": "object",
            "properties": Value::Object(req_props),
            "required": req_required,
        }
    })
}

/// Build the `/api/{name}` and `/api/{name}/{id}` path items for a type.
pub fn content_type_paths(ct: &ContentType) -> Value {
    let (resp_name, req_name) = schema_names(&ct.name);
    let resp_ref = format!("#/components/schemas/{resp_name}");
    let req_ref = format!("#/components/schemas/{req_name}");
    let tag = ct.display_name.clone();
    let secured = json!([{ "bearerAuth": [] }]);
    let errs = json!({
        "401": { "$ref": "#/components/responses/Unauthorized" },
        "403": { "$ref": "#/components/responses/Forbidden" },
        "404": { "$ref": "#/components/responses/NotFound" }
    });

    let list_get = json!({
        "tags": [tag],
        "summary": format!("List {} entries", ct.name),
        "security": secured,
        "parameters": [
            { "name": "page", "in": "query", "schema": { "type": "integer" } },
            { "name": "pageSize", "in": "query", "schema": { "type": "integer" } },
            { "name": "sort", "in": "query", "schema": { "type": "string" } },
            { "name": "populate", "in": "query", "schema": { "type": "string" } }
        ],
        "responses": merge_obj(json!({
            "200": {
                "description": "List of entries",
                "content": { "application/json": { "schema": {
                    "type": "object",
                    "required": ["data", "meta"],
                    "properties": {
                        "data": { "type": "array", "items": { "$ref": resp_ref } },
                        "meta": { "type": "object", "required": ["page", "pageSize", "total"], "properties": {
                            "page": { "type": "integer" },
                            "pageSize": { "type": "integer" },
                            "total": { "type": "integer" }
                        }}
                    }
                }}}
            }
        }), errs.clone())
    });

    let create_post = json!({
        "tags": [tag],
        "summary": format!("Create a {} entry", ct.name),
        "security": secured,
        "requestBody": { "required": true, "content": { "application/json": {
            "schema": { "$ref": req_ref }
        }}},
        "responses": merge_obj(json!({
            "201": { "description": "Created", "content": { "application/json": {
                "schema": { "$ref": resp_ref }
            }}}
        }), errs.clone())
    });

    let id_param = json!([{
        "name": "id", "in": "path", "required": true,
        "schema": { "type": "string", "format": "uuid" }
    }]);

    // get_one also accepts ?populate= (handled by content::get_one).
    let get_one_params = json!([
        { "name": "id", "in": "path", "required": true,
          "schema": { "type": "string", "format": "uuid" } },
        { "name": "populate", "in": "query", "schema": { "type": "string" } }
    ]);
    let get_one = json!({
        "tags": [tag], "summary": format!("Fetch one {} entry", ct.name),
        "security": secured, "parameters": get_one_params,
        "responses": merge_obj(json!({
            "200": { "description": "Entry", "content": { "application/json": {
                "schema": { "$ref": resp_ref }
            }}}
        }), errs.clone())
    });

    let put_one = json!({
        "tags": [tag], "summary": format!("Replace a {} entry", ct.name),
        "security": secured, "parameters": id_param,
        "requestBody": { "required": true, "content": { "application/json": {
            "schema": { "$ref": req_ref }
        }}},
        "responses": merge_obj(json!({
            "200": { "description": "Updated", "content": { "application/json": {
                "schema": { "$ref": resp_ref }
            }}}
        }), errs.clone())
    });

    let delete_one = json!({
        "tags": [tag], "summary": format!("Delete a {} entry", ct.name),
        "security": secured, "parameters": id_param,
        "responses": merge_obj(json!({ "204": { "description": "Deleted" } }), errs)
    });

    json!({
        format!("/api/{}", ct.name): {
            "get": list_get,
            "post": create_post
        },
        format!("/api/{}/{{id}}", ct.name): {
            "get": get_one,
            "put": put_one,
            "delete": delete_one
        }
    })
}

/// Shallow-merge two JSON objects (right wins on key conflict).
fn merge_obj(mut base: Value, extra: Value) -> Value {
    if let (Value::Object(ref mut b), Value::Object(e)) = (&mut base, extra) {
        for (k, v) in e {
            b.insert(k, v);
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustapi_core::field::Field;
    use rustapi_core::ContentType;
    use chrono::Utc;
    use uuid::Uuid;
    use serde_json::json;

    fn f(kind: FieldKind, kind_meta: Value) -> Field {
        Field {
            name: "x".into(),
            kind,
            required: false,
            unique: false,
            default: Value::Null,
            max_length: None,
            kind_meta,
        }
    }

    #[test]
    fn string_has_maxlength() {
        let s = field_to_schema(&f(FieldKind::String, json!({})));
        assert_eq!(s["type"], "string");
        assert_eq!(s["maxLength"], 255);
    }

    #[test]
    fn integer_float_bool() {
        assert_eq!(field_to_schema(&f(FieldKind::Integer, json!({})))["format"], "int64");
        assert_eq!(field_to_schema(&f(FieldKind::Float, json!({})))["format"], "double");
        assert_eq!(field_to_schema(&f(FieldKind::Boolean, json!({})))["type"], "boolean");
    }

    #[test]
    fn datetime_uuid_email_url() {
        assert_eq!(field_to_schema(&f(FieldKind::Datetime, json!({})))["format"], "date-time");
        assert_eq!(field_to_schema(&f(FieldKind::Uuid, json!({})))["format"], "uuid");
        assert_eq!(field_to_schema(&f(FieldKind::Email, json!({})))["format"], "email");
        assert_eq!(field_to_schema(&f(FieldKind::Url, json!({})))["format"], "uri");
    }

    #[test]
    fn slug_has_pattern() {
        let s = field_to_schema(&f(FieldKind::Slug, json!({})));
        assert_eq!(s["pattern"], "^[a-z0-9]+(?:-[a-z0-9]+)*$");
    }

    #[test]
    fn enum_lists_values() {
        let s = field_to_schema(&f(FieldKind::Enum, json!({ "values": ["draft", "published"] })));
        assert_eq!(s["enum"], json!(["draft", "published"]));
    }

    #[test]
    fn json_is_any() {
        assert_eq!(field_to_schema(&f(FieldKind::Json, json!({}))), json!({}));
    }

    #[test]
    fn relation_single_vs_many() {
        let one = field_to_schema(&f(FieldKind::Relation, json!({ "target": "user", "cardinality": "many_to_one" })));
        assert_eq!(one["format"], "uuid");
        let many = field_to_schema(&f(FieldKind::Relation, json!({ "target": "tag", "cardinality": "many_to_many" })));
        assert_eq!(many["type"], "array");
    }

    #[test]
    fn media_single_vs_multiple() {
        let one = field_to_schema(&f(FieldKind::Media, json!({ "multiple": false })));
        assert_eq!(one["format"], "uuid");
        let many = field_to_schema(&f(FieldKind::Media, json!({ "multiple": true })));
        assert_eq!(many["type"], "array");
    }

    #[test]
    fn default_is_emitted() {
        let mut field = f(FieldKind::Integer, json!({}));
        field.default = json!(7);
        assert_eq!(field_to_schema(&field)["default"], json!(7));
    }

    #[test]
    fn text_no_maxlength_by_default() {
        let s = field_to_schema(&f(FieldKind::Text, json!({})));
        assert_eq!(s["type"], "string");
        assert!(s["maxLength"].is_null());
    }

    #[test]
    fn text_with_explicit_maxlength() {
        let mut field = f(FieldKind::Text, json!({}));
        field.max_length = Some(5000);
        let s = field_to_schema(&field);
        assert_eq!(s["maxLength"], 5000);
    }

    #[test]
    fn enum_empty_meta_no_enum_key() {
        let s = field_to_schema(&f(FieldKind::Enum, json!({})));
        assert_eq!(s["type"], "string");
        assert!(s["enum"].is_null());
    }

    fn sample_ct() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "article".into(),
            display_name: "Article".into(),
            fields: vec![
                Field { name: "title".into(), kind: FieldKind::String, required: true, unique: false, default: Value::Null, max_length: None, kind_meta: json!({}) },
                Field { name: "views".into(), kind: FieldKind::Integer, required: false, unique: false, default: Value::Null, max_length: None, kind_meta: json!({}) },
            ],
            options: json!({}),
            kind: rustapi_core::ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn schema_names_pascalcase() {
        assert_eq!(schema_names("blog_post"), ("BlogPost".into(), "BlogPostInput".into()));
    }

    #[test]
    fn response_schema_has_system_fields_request_does_not() {
        let s = content_type_schemas(&sample_ct());
        let resp = &s["Article"]["properties"];
        let req = &s["ArticleInput"]["properties"];
        assert!(resp["id"].is_object());
        assert!(resp["created_at"].is_object());
        assert!(resp["title"].is_object());
        assert!(req["title"].is_object());
        assert!(req["id"].is_null(), "request schema must omit id");
        assert!(req["created_at"].is_null(), "request schema must omit timestamps");
    }

    #[test]
    fn required_field_listed_in_both() {
        let s = content_type_schemas(&sample_ct());
        assert!(s["Article"]["required"].as_array().unwrap().iter().any(|v| v == "title"));
        assert!(s["ArticleInput"]["required"].as_array().unwrap().iter().any(|v| v == "title"));
    }

    #[test]
    fn paths_cover_list_and_item() {
        let p = content_type_paths(&sample_ct());
        assert!(p["/api/article"]["get"].is_object());
        assert!(p["/api/article"]["post"].is_object());
        assert!(p["/api/article/{id}"]["get"].is_object());
        assert!(p["/api/article/{id}"]["put"].is_object());
        assert!(p["/api/article/{id}"]["delete"].is_object());
        // get_one documents the ?populate= query param.
        let get_params = p["/api/article/{id}"]["get"]["parameters"].as_array().unwrap();
        assert!(get_params.iter().any(|prm| prm["name"] == "populate" && prm["in"] == "query"));

        let env = &p["/api/article"]["get"]["responses"]["200"]["content"]["application/json"]["schema"];
        assert_eq!(env["required"], serde_json::json!(["data", "meta"]));
        assert_eq!(env["properties"]["meta"]["required"], serde_json::json!(["page", "pageSize", "total"]));
        assert_eq!(p["/api/article"]["get"]["tags"], serde_json::json!(["Article"]));
    }
}
