//! Builds an `async_graphql::dynamic::Schema` at runtime from the content-type
//! registry. An output object is registered for EVERY content type (incl.
//! Single) so relation fields can target any type without dangling; a shared
//! `Media` object is registered for media fields. Only Collection types get
//! root Query/Mutation fields — Single types are not queryable as collections
//! in v1, but their object still exists as a relation target.
//!
//! Field/query/mutation resolvers come from `resolve::`: object/envelope/Meta/
//! Media fields use `json_field_resolver` (parent JSON threaded to children),
//! root query/mutation fields delegate to the shared `content::` CRUD. SDL
//! generation does not invoke resolvers, so the `.sdl()` tests below exercise
//! only the shape.

use std::collections::HashSet;

use async_graphql::dynamic::{
    Enum, Field, FieldFuture, InputObject, InputValue, Object, ResolverContext, Scalar, Schema,
    SchemaBuilder, SchemaError, TypeRef,
};
use rustapi_core::field::FieldKind;
use rustapi_core::{ContentType, ContentTypeKind};

use crate::graphql::{resolve, scalars};

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

// Inert resolver for the `_empty` placeholder fields (only present when no
// Collection types are surfaced). Always yields null.
fn empty_resolver() -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    |_ctx: ResolverContext| FieldFuture::new(async { Ok(None::<async_graphql::Value>) })
}

/// Output object for a content type: system fields + one per content field.
/// All output `Field::new` resolver call sites live here (Task 5 swaps them).
fn build_output_object(ct: &ContentType) -> Object {
    let mut object = Object::new(pascal(&ct.name))
        .field(Field::new(
            "id",
            TypeRef::named_nn(scalars::UUID_SCALAR),
            resolve::json_field_resolver("id"),
        ))
        .field(Field::new(
            "created_at",
            TypeRef::named_nn(scalars::DATETIME_SCALAR),
            resolve::json_field_resolver("created_at"),
        ))
        .field(Field::new(
            "updated_at",
            TypeRef::named_nn(scalars::DATETIME_SCALAR),
            resolve::json_field_resolver("updated_at"),
        ));
    for field in &ct.fields {
        object = object.field(Field::new(
            &field.name,
            scalars::field_type_ref(field),
            resolve::json_field_resolver(&field.name),
        ));
    }
    object
}

/// Input object for a content type: writable fields. List-valued fields (m2m
/// relation, multiple media) accept lists, matching the output read shape.
fn build_input_object(ct: &ContentType) -> InputObject {
    let mut input = InputObject::new(format!("{}Input", pascal(&ct.name)));
    for field in &ct.fields {
        input = input.field(InputValue::new(&field.name, scalars::input_type_ref(field)));
    }
    input
}

/// List envelope object (`<Type>List`): paginated `data` + `meta`.
fn build_list_envelope(type_name: &str) -> Object {
    Object::new(format!("{type_name}List"))
        .field(Field::new(
            "data",
            TypeRef::named_nn_list_nn(type_name),
            resolve::json_field_resolver("data"),
        ))
        .field(Field::new(
            "meta",
            TypeRef::named_nn("Meta"),
            resolve::json_field_resolver("meta"),
        ))
}

/// Register one Enum type per distinct enum field, deduped by name across types.
fn register_enums(
    mut builder: SchemaBuilder,
    ct: &ContentType,
    seen: &mut HashSet<String>,
) -> SchemaBuilder {
    for field in &ct.fields {
        if field.kind != FieldKind::Enum {
            continue;
        }
        let enum_name = scalars::enum_type_name(field);
        if !seen.insert(enum_name.clone()) {
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
    builder
}

/// Build a dynamic GraphQL schema from the content-type registry. An output
/// object is registered for EVERY content type (so relation fields can target
/// any type, incl. Single, without dangling); only `Collection` types get root
/// Query/Mutation fields. A shared `Media` object backs media fields.
pub fn build_schema(types: &[ContentType]) -> Result<Schema, SchemaError> {
    let mut builder = Schema::build("Query", Some("Mutation"), None);

    // Shared, registered once: custom scalars, the Meta envelope, and the
    // Media object. Relation fields are typed as the target type's object and
    // media fields as `Media` (see `scalars::base_type_name`).
    builder = builder
        .register(Scalar::new(scalars::UUID_SCALAR))
        .register(Scalar::new(scalars::DATETIME_SCALAR))
        .register(Scalar::new(scalars::JSON_SCALAR));

    // Shared Media object. Media fields embed a media object into the row JSON
    // (see media_embed), so children read the AssetView keys from the parent
    // value. `size_bytes` is i64 in AssetView; async-graphql's Int is i32, so it
    // would overflow above 2GB — expose it via the JSON scalar to pass the raw
    // number through without coercion.
    let media = Object::new("Media")
        .field(Field::new(
            "id",
            TypeRef::named_nn(scalars::UUID_SCALAR),
            resolve::json_field_resolver("id"),
        ))
        .field(Field::new(
            "file_name",
            TypeRef::named_nn(TypeRef::STRING),
            resolve::json_field_resolver("file_name"),
        ))
        .field(Field::new(
            "original_filename",
            TypeRef::named_nn(TypeRef::STRING),
            resolve::json_field_resolver("original_filename"),
        ))
        .field(Field::new(
            "mime_type",
            TypeRef::named_nn(TypeRef::STRING),
            resolve::json_field_resolver("mime_type"),
        ))
        .field(Field::new(
            "size_bytes",
            TypeRef::named_nn(scalars::JSON_SCALAR),
            resolve::json_field_resolver("size_bytes"),
        ))
        .field(Field::new(
            "width",
            TypeRef::named(TypeRef::INT),
            resolve::json_field_resolver("width"),
        ))
        .field(Field::new(
            "height",
            TypeRef::named(TypeRef::INT),
            resolve::json_field_resolver("height"),
        ))
        .field(Field::new(
            "alt_text",
            TypeRef::named(TypeRef::STRING),
            resolve::json_field_resolver("alt_text"),
        ))
        .field(Field::new(
            "caption",
            TypeRef::named(TypeRef::STRING),
            resolve::json_field_resolver("caption"),
        ));
    builder = builder.register(media);

    let meta = Object::new("Meta")
        .field(Field::new(
            "page",
            TypeRef::named_nn(TypeRef::INT),
            resolve::json_field_resolver("page"),
        ))
        .field(Field::new(
            "pageSize",
            TypeRef::named_nn(TypeRef::INT),
            resolve::json_field_resolver("pageSize"),
        ))
        .field(Field::new(
            "total",
            TypeRef::named_nn(TypeRef::INT),
            resolve::json_field_resolver("total"),
        ));
    builder = builder.register(meta);

    let mut query = Object::new("Query");
    let mut mutation = Object::new("Mutation");
    let mut registered_enums: HashSet<String> = HashSet::new();
    let mut surfaced_any = false;

    for ct in types.iter() {
        let type_name = pascal(&ct.name);

        // Output object is registered for EVERY type so relation fields can
        // reference Single-type targets without dangling. Each type's object is
        // registered exactly once here.
        builder = builder.register(build_output_object(ct));
        builder = register_enums(builder, ct, &mut registered_enums);

        // Collections also get an input, list envelope, and root Query/Mutation
        // fields. Single types are not queryable as collections in v1.
        if ct.kind != ContentTypeKind::Collection {
            continue;
        }
        surfaced_any = true;
        let input_name = format!("{type_name}Input");
        let list_name = format!("{type_name}List");

        builder = builder
            .register(build_input_object(ct))
            .register(build_list_envelope(&type_name));

        // Query: list + single.
        query = query.field(
            Field::new(
                plural(&ct.name),
                TypeRef::named_nn(&list_name),
                resolve::list_field(ct.name.clone()),
            )
            .argument(InputValue::new("page", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("pageSize", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("sort", TypeRef::named(TypeRef::STRING)))
            .argument(InputValue::new(
                "filters",
                TypeRef::named(scalars::JSON_SCALAR),
            ))
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING))),
        );
        query = query.field(
            Field::new(
                camel(&ct.name),
                TypeRef::named(&type_name),
                resolve::get_field(ct.name.clone()),
            )
            .argument(InputValue::new(
                "id",
                TypeRef::named_nn(scalars::UUID_SCALAR),
            ))
            .argument(InputValue::new("locale", TypeRef::named(TypeRef::STRING))),
        );

        // Mutation: create / update / delete.
        mutation = mutation.field(
            Field::new(
                format!("create{type_name}"),
                TypeRef::named_nn(&type_name),
                resolve::create_field(ct.name.clone()),
            )
            .argument(InputValue::new("data", TypeRef::named_nn(&input_name))),
        );
        mutation = mutation.field(
            Field::new(
                format!("update{type_name}"),
                TypeRef::named_nn(&type_name),
                resolve::update_field(ct.name.clone()),
            )
            .argument(InputValue::new(
                "id",
                TypeRef::named_nn(scalars::UUID_SCALAR),
            ))
            .argument(InputValue::new("data", TypeRef::named_nn(&input_name))),
        );
        mutation = mutation.field(
            Field::new(
                format!("delete{type_name}"),
                TypeRef::named_nn(TypeRef::BOOLEAN),
                resolve::delete_field(ct.name.clone()),
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
            empty_resolver(),
        ));
        mutation = mutation.field(Field::new(
            "_empty",
            TypeRef::named(TypeRef::BOOLEAN),
            empty_resolver(),
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
    fn single_type_object_registered_no_root_field() {
        let mut s = article();
        s.name = "homepage".into();
        s.kind = ContentTypeKind::Single;
        let schema = build_schema(&[s]).expect("build");
        let sdl = schema.sdl();
        // object IS registered (so relations can target it)...
        assert!(
            sdl.contains("type Homepage"),
            "single object registered: {sdl}"
        );
        // ...but NO root collection field for it
        assert!(
            !sdl.contains("homepages("),
            "single type has no list query: {sdl}"
        );
    }

    #[test]
    fn relation_to_single_target_builds() {
        // Single target
        let mut home = article();
        home.name = "homepage".into();
        home.kind = ContentTypeKind::Single;
        // Collection with a relation to the Single
        let mut banner = article();
        banner.name = "banner".into();
        banner.kind = ContentTypeKind::Collection;
        banner.fields = vec![Field {
            name: "page".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: Value::Null,
            max_length: None,
            kind_meta: json!({ "target": "homepage", "cardinality": "many_to_one" }),
        }];
        // must NOT error (previously dangling ref → Err)
        let schema = build_schema(&[home, banner]).expect("schema with relation to single builds");
        let sdl = schema.sdl();
        assert!(sdl.contains("type Banner"));
        assert!(
            sdl.contains("page: Homepage"),
            "relation field typed as target object: {sdl}"
        );
    }
}
