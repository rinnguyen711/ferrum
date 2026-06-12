//! Builds an `async_graphql::dynamic::Schema` at runtime from the content-type
//! registry. Walks Collection types only (Single types are excluded from v1).
//!
//! Field/query/mutation resolvers are TEMP stubs here — Task 5 swaps them for
//! the real `resolve::` factories. SDL generation does not invoke resolvers, so
//! the shape is fully exercised by the `.sdl()` tests below.

use std::collections::HashSet;

use async_graphql::dynamic::{
    Enum, Field, FieldFuture, InputObject, InputValue, Object, ResolverContext, Scalar, Schema,
    SchemaError, TypeRef,
};
use rustapi_core::field::FieldKind;
use rustapi_core::{ContentType, ContentTypeKind};

use crate::graphql::scalars;

/// PascalCase a snake_case api id (`blog_post` -> `BlogPost`). Same rule as
/// `openapi::schema::to_pascal`.
pub fn pascal(name: &str) -> String {
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

/// camelCase root + naive pluralize for the list query name. "y"->"ies", else append "s".
pub fn plural(name: &str) -> String {
    let p = pascal(name);
    let lower = p[..1].to_lowercase() + &p[1..];
    if let Some(stem) = lower.strip_suffix('y') {
        format!("{stem}ies")
    } else {
        format!("{lower}s")
    }
}

/// camelCase a PascalCase name for the single-item query (`Article` -> `article`).
fn camel(name: &str) -> String {
    let p = pascal(name);
    p[..1].to_lowercase() + &p[1..]
}

// TEMP stub — replaced by resolve.rs wiring in Task 5. SDL generation never
// invokes resolvers, so a no-op that yields null is sufficient here.
fn stub_resolver() -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    |_ctx: ResolverContext| FieldFuture::new(async { Ok(None::<async_graphql::Value>) })
}

/// Build a dynamic GraphQL schema from the content-type registry. Only
/// `Collection` types are surfaced in v1; `Single` types are skipped.
pub fn build_schema(types: &[ContentType]) -> Result<Schema, SchemaError> {
    let mut builder = Schema::build("Query", Some("Mutation"), None);

    // Shared, registered once.
    builder = builder
        .register(Scalar::new(scalars::UUID_SCALAR))
        .register(Scalar::new(scalars::DATETIME_SCALAR))
        .register(Scalar::new(scalars::JSON_SCALAR));

    let meta = Object::new("Meta")
        .field(Field::new(
            "page",
            TypeRef::named_nn(TypeRef::INT),
            stub_resolver(),
        ))
        .field(Field::new(
            "pageSize",
            TypeRef::named_nn(TypeRef::INT),
            stub_resolver(),
        ))
        .field(Field::new(
            "total",
            TypeRef::named_nn(TypeRef::INT),
            stub_resolver(),
        ));
    builder = builder.register(meta);

    let media = Object::new("Media")
        .field(Field::new(
            "id",
            TypeRef::named_nn(scalars::UUID_SCALAR),
            stub_resolver(),
        ))
        .field(Field::new(
            "url",
            TypeRef::named(TypeRef::STRING),
            stub_resolver(),
        ));
    builder = builder.register(media);

    let mut query = Object::new("Query");
    let mut mutation = Object::new("Mutation");
    let mut registered_enums: HashSet<String> = HashSet::new();
    let mut surfaced_any = false;

    for ct in types
        .iter()
        .filter(|ct| ct.kind == ContentTypeKind::Collection)
    {
        surfaced_any = true;
        let type_name = pascal(&ct.name);
        let input_name = format!("{type_name}Input");
        let list_name = format!("{type_name}List");

        // Output object: system fields + one per content field.
        let mut object = Object::new(&type_name)
            .field(Field::new(
                "id",
                TypeRef::named_nn(scalars::UUID_SCALAR),
                stub_resolver(),
            ))
            .field(Field::new(
                "created_at",
                TypeRef::named_nn(scalars::DATETIME_SCALAR),
                stub_resolver(),
            ))
            .field(Field::new(
                "updated_at",
                TypeRef::named_nn(scalars::DATETIME_SCALAR),
                stub_resolver(),
            ));
        for field in &ct.fields {
            object = object.field(Field::new(
                &field.name,
                scalars::field_type_ref(field),
                stub_resolver(),
            ));
        }
        builder = builder.register(object);

        // Input object: writable fields.
        let mut input = InputObject::new(&input_name);
        for field in &ct.fields {
            let base = scalars::base_type_name(field);
            let ty = if field.required {
                TypeRef::named_nn(base)
            } else {
                TypeRef::named(base)
            };
            input = input.field(InputValue::new(&field.name, ty));
        }
        builder = builder.register(input);

        // List envelope.
        let list = Object::new(&list_name)
            .field(Field::new(
                "data",
                TypeRef::named_nn_list_nn(&type_name),
                stub_resolver(),
            ))
            .field(Field::new(
                "meta",
                TypeRef::named_nn("Meta"),
                stub_resolver(),
            ));
        builder = builder.register(list);

        // One Enum per distinct enum field, deduped by name.
        for field in &ct.fields {
            if field.kind != FieldKind::Enum {
                continue;
            }
            let enum_name = scalars::enum_type_name(field);
            if !registered_enums.insert(enum_name.clone()) {
                continue;
            }
            let Some(meta) = field.enum_meta() else {
                continue;
            };
            let mut e = Enum::new(&enum_name);
            for v in &meta.values {
                e = e.item(v.as_str());
            }
            builder = builder.register(e);
        }

        // Query: list + single.
        query = query.field(
            Field::new(
                plural(&ct.name),
                TypeRef::named_nn(&list_name),
                stub_resolver(),
            )
            .argument(InputValue::new("page", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("pageSize", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("sort", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new(
                "filters",
                TypeRef::named(scalars::JSON_SCALAR),
            )),
        );
        query = query.field(
            Field::new(camel(&ct.name), TypeRef::named(&type_name), stub_resolver()).argument(
                InputValue::new("id", TypeRef::named_nn(scalars::UUID_SCALAR)),
            ),
        );

        // Mutation: create / update / delete.
        mutation = mutation.field(
            Field::new(
                format!("create{type_name}"),
                TypeRef::named_nn(&type_name),
                stub_resolver(),
            )
            .argument(InputValue::new(
                "data",
                TypeRef::named_nn(&input_name),
            )),
        );
        mutation = mutation.field(
            Field::new(
                format!("update{type_name}"),
                TypeRef::named_nn(&type_name),
                stub_resolver(),
            )
            .argument(InputValue::new(
                "id",
                TypeRef::named_nn(scalars::UUID_SCALAR),
            ))
            .argument(InputValue::new(
                "data",
                TypeRef::named_nn(&input_name),
            )),
        );
        mutation = mutation.field(
            Field::new(
                format!("delete{type_name}"),
                TypeRef::named_nn(TypeRef::BOOLEAN),
                stub_resolver(),
            )
            .argument(InputValue::new(
                "id",
                TypeRef::named_nn(scalars::UUID_SCALAR),
            )),
        );
    }

    // GraphQL requires Query and Mutation to define at least one field. When no
    // Collection types are surfaced, add inert placeholders so the schema builds.
    if !surfaced_any {
        query = query.field(Field::new(
            "_empty",
            TypeRef::named(TypeRef::BOOLEAN),
            stub_resolver(),
        ));
        mutation = mutation.field(Field::new(
            "_empty",
            TypeRef::named(TypeRef::BOOLEAN),
            stub_resolver(),
        ));
    }

    builder.register(query).register(mutation).finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::field::{Field, FieldKind};
    use rustapi_core::{ContentType, ContentTypeKind};
    use serde_json::{json, Value};
    use uuid::Uuid;

    fn article() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "article".into(),
            display_name: "Article".into(),
            fields: vec![
                Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: true,
                    unique: false,
                    default: Value::Null,
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "views".into(),
                    kind: FieldKind::Integer,
                    required: false,
                    unique: false,
                    default: Value::Null,
                    max_length: None,
                    kind_meta: json!({}),
                },
            ],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn schema_has_type_query_mutation_for_collection() {
        let schema = build_schema(&[article()]).expect("build");
        let sdl = schema.sdl();
        assert!(sdl.contains("type Article"), "{sdl}");
        assert!(sdl.contains("input ArticleInput"), "{sdl}");
        assert!(sdl.contains("title: String!"), "{sdl}");
        assert!(sdl.contains("views: Int"), "{sdl}");
        assert!(sdl.contains("articles("), "{sdl}");
        assert!(sdl.contains("article("), "{sdl}");
        assert!(sdl.contains("createArticle("), "{sdl}");
        assert!(sdl.contains("updateArticle("), "{sdl}");
        assert!(sdl.contains("deleteArticle("), "{sdl}");
    }

    #[test]
    fn single_type_is_skipped() {
        let mut s = article();
        s.name = "homepage".into();
        s.kind = ContentTypeKind::Single;
        let schema = build_schema(&[s]).expect("build");
        assert!(
            !schema.sdl().contains("homepages("),
            "single types excluded from v1"
        );
    }
}
