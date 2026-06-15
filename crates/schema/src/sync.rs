//! Declarative schema sync: load content types from TOML file(s) and reconcile
//! the database to match on startup. See
//! docs/superpowers/specs/2026-06-14-schema-as-code-toml-sync-design.md.

use std::path::Path;

use rustapi_core::{ContentType, Error, Field, NewContentType, ValidationErrors};
use serde::Deserialize;

/// How aggressively sync reconciles the DB toward the TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Create missing types, add missing fields. Never drop. (default)
    #[default]
    Additive,
    /// Also drop types/fields absent from the TOML.
    Full,
}

impl SyncMode {
    /// Parse from the `RUSTAPI_SCHEMA_SYNC` env value. Unknown/empty → Additive.
    pub fn from_env_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "full" => SyncMode::Full,
            _ => SyncMode::Additive,
        }
    }
}

/// One TOML file's worth of content types and components.
#[derive(Debug, Deserialize)]
struct SchemaFile {
    #[serde(default, rename = "content_type")]
    content_types: Vec<TomlContentType>,
    #[serde(default, rename = "component")]
    components: Vec<TomlComponent>,
}

/// A component as declared in TOML. `field` is renamed so the TOML key is
/// `[[component.field]]`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct TomlComponent {
    pub uid: String,
    pub display_name: String,
    #[serde(default, rename = "field")]
    pub fields: Vec<Field>,
}

/// Parsed content of one or more TOML schema documents.
pub(crate) struct ParsedSchema {
    pub content_types: Vec<NewContentType>,
    pub components: Vec<TomlComponent>,
}

/// Parse a single TOML document into content types + components.
pub(crate) fn parse_schema(doc: &str) -> Result<ParsedSchema, Error> {
    let parsed: SchemaFile = toml::from_str(doc).map_err(|e| {
        Error::Validation(ValidationErrors::single(format!("schema TOML parse: {e}")))
    })?;
    Ok(ParsedSchema {
        content_types: parsed.content_types.into_iter().map(Into::into).collect(),
        components: parsed.components,
    })
}

/// A content type as declared in TOML. Maps onto `NewContentType`; `field` is
/// renamed so the TOML key is `[[content_type.field]]`.
#[derive(Debug, Deserialize)]
struct TomlContentType {
    name: String,
    display_name: String,
    #[serde(default)]
    kind: rustapi_core::ContentTypeKind,
    #[serde(default)]
    options: serde_json::Value,
    #[serde(default, rename = "field")]
    fields: Vec<Field>,
}

impl From<TomlContentType> for NewContentType {
    fn from(t: TomlContentType) -> Self {
        NewContentType {
            name: t.name,
            display_name: t.display_name,
            fields: t.fields,
            options: t.options,
            kind: t.kind,
        }
    }
}

/// One reconciliation step computed by `plan_sync`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SyncAction {
    /// Type in TOML, absent from DB.
    Create(NewContentType),
    /// Type in both: add these fields, drop these field names (drops only in Full).
    Patch {
        name: String,
        add_fields: Vec<Field>,
        drop_fields: Vec<String>,
        options: serde_json::Value,
    },
    /// Type in DB, absent from TOML (Full mode only).
    DropType(String),
    /// Type in DB, absent from TOML (Additive): clear its `managed` flag.
    Unmanage(String),
}

/// Compute the reconciliation plan. Pure: no DB. `desired` is the TOML set,
/// `current` the live registry list. Returns actions in no particular order;
/// the apply loop orders creates by relation dependency.
pub(crate) fn plan_sync(
    desired: &[NewContentType],
    current: &[ContentType],
    mode: SyncMode,
) -> Result<Vec<SyncAction>, Error> {
    use std::collections::HashMap;
    let cur: HashMap<&str, &ContentType> = current.iter().map(|c| (c.name.as_str(), c)).collect();
    let des: HashMap<&str, &NewContentType> =
        desired.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut actions = Vec::new();

    for d in desired {
        match cur.get(d.name.as_str()) {
            None => actions.push(SyncAction::Create(d.clone())),
            Some(existing) => {
                let cur_fields: HashMap<&str, &Field> = existing
                    .fields
                    .iter()
                    .map(|f| (f.name.as_str(), f))
                    .collect();
                let des_fields: HashMap<&str, &Field> =
                    d.fields.iter().map(|f| (f.name.as_str(), f)).collect();

                let mut add_fields = Vec::new();
                for f in &d.fields {
                    match cur_fields.get(f.name.as_str()) {
                        None => add_fields.push(f.clone()),
                        Some(cf) => {
                            if cf.kind != f.kind || cf.kind_meta != f.kind_meta {
                                return Err(Error::Validation(ValidationErrors::field(
                                    &f.name,
                                    format!(
                                        "field `{}` on `{}` changed kind/meta; not supported \
                                         (drop+add in full mode, or edit in UI)",
                                        f.name, d.name
                                    ),
                                )));
                            }
                        }
                    }
                }

                let mut drop_fields = Vec::new();
                if mode == SyncMode::Full {
                    for f in &existing.fields {
                        if !des_fields.contains_key(f.name.as_str())
                            && !rustapi_core::is_system_column(&f.name)
                        {
                            drop_fields.push(f.name.clone());
                        }
                    }
                }

                actions.push(SyncAction::Patch {
                    name: d.name.clone(),
                    add_fields,
                    drop_fields,
                    options: managed_options(&d.resolved_options()),
                });
            }
        }
    }

    for c in current {
        if !des.contains_key(c.name.as_str()) {
            match mode {
                SyncMode::Full => actions.push(SyncAction::DropType(c.name.clone())),
                SyncMode::Additive => {
                    if c.managed() {
                        actions.push(SyncAction::Unmanage(c.name.clone()));
                    }
                }
            }
        }
    }

    Ok(actions)
}

/// Merge `managed = true` into a type's declared options.
fn managed_options(declared: &serde_json::Value) -> serde_json::Value {
    let mut obj = declared.as_object().cloned().unwrap_or_default();
    obj.insert("managed".into(), serde_json::Value::Bool(true));
    serde_json::Value::Object(obj)
}

/// Load + merge all content types and components from a path. If `path` is a
/// directory, every `*.toml` file in it (non-recursive) is parsed and merged.
/// If a file, that one file is parsed. Duplicate type names or component uids
/// across files are rejected.
pub(crate) fn load_desired(path: &Path) -> Result<ParsedSchema, Error> {
    let mut docs: Vec<(String, String)> = Vec::new();
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("read schema dir {path:?}: {e}")))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
            .collect();
        entries.sort();
        for p in entries {
            let body = std::fs::read_to_string(&p)
                .map_err(|e| Error::Internal(anyhow::anyhow!("read {p:?}: {e}")))?;
            docs.push((p.display().to_string(), body));
        }
    } else {
        let body = std::fs::read_to_string(path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("read {path:?}: {e}")))?;
        docs.push((path.display().to_string(), body));
    }

    let mut content_types: Vec<NewContentType> = Vec::new();
    let mut components: Vec<TomlComponent> = Vec::new();
    let mut seen_ct: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_comp: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (label, body) in docs {
        let parsed = parse_schema(&body)?;
        for ct in parsed.content_types {
            if !seen_ct.insert(ct.name.clone()) {
                return Err(Error::Validation(ValidationErrors::single(format!(
                    "duplicate content type `{}` (in {label})",
                    ct.name
                ))));
            }
            content_types.push(ct);
        }
        for c in parsed.components {
            if !seen_comp.insert(c.uid.clone()) {
                return Err(Error::Validation(ValidationErrors::single(format!(
                    "duplicate component `{}` (in {label})",
                    c.uid
                ))));
            }
            components.push(c);
        }
    }
    Ok(ParsedSchema {
        content_types,
        components,
    })
}

/// Order creates so a relation's target is created before the dependent type.
/// Stable topological sort by relation targets; self-references and cycles fall
/// back to declaration order (DB-level checks still apply at apply time).
fn order_creates(mut creates: Vec<NewContentType>) -> Vec<NewContentType> {
    use std::collections::HashSet;
    let names: HashSet<String> = creates.iter().map(|c| c.name.clone()).collect();
    let mut ordered: Vec<NewContentType> = Vec::with_capacity(creates.len());
    let mut placed: HashSet<String> = HashSet::new();

    while !creates.is_empty() {
        let idx = creates.iter().position(|c| {
            c.fields.iter().all(|f| match f.relation_meta() {
                Some(m) => {
                    m.target == c.name || !names.contains(&m.target) || placed.contains(&m.target)
                }
                None => true,
            })
        });
        match idx {
            Some(i) => {
                let c = creates.remove(i);
                placed.insert(c.name.clone());
                ordered.push(c);
            }
            None => {
                ordered.append(&mut creates);
                break;
            }
        }
    }
    ordered
}

/// Order type drops so a type that holds a relation to another to-be-dropped
/// type is deleted before its target (reverse of create ordering). Prevents
/// FK-RESTRICT violations when dropping related types in full mode.
fn order_drops(mut names: Vec<String>, current: &[ContentType]) -> Vec<String> {
    use std::collections::{HashMap, HashSet};
    let by_name: HashMap<&str, &ContentType> =
        current.iter().map(|c| (c.name.as_str(), c)).collect();
    let drop_set: HashSet<String> = names.iter().cloned().collect();
    let mut ordered: Vec<String> = Vec::with_capacity(names.len());
    let mut placed: HashSet<String> = HashSet::new();

    while !names.is_empty() {
        // Pick a type none of whose still-unplaced relation targets (within the
        // drop set) remain — i.e. a leaf in the "depends on" graph: a type that
        // is not depended-upon by any remaining type. We drop dependents first,
        // so a type is droppable when no OTHER remaining type relates to it.
        let idx = names.iter().position(|n| {
            !names.iter().any(|other| {
                if other == n || placed.contains(other) {
                    return false;
                }
                by_name
                    .get(other.as_str())
                    .map(|ct| {
                        ct.fields.iter().any(|f| {
                            f.relation_meta()
                                .map(|m| m.target == *n && drop_set.contains(&m.target))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        });
        match idx {
            Some(i) => {
                let n = names.remove(i);
                placed.insert(n.clone());
                ordered.push(n);
            }
            None => {
                // Cycle (e.g. mutual relations): fall back to declaration order.
                ordered.append(&mut names);
                break;
            }
        }
    }
    ordered
}

/// Entry point called at boot. Loads TOML from `path`, diffs against the live
/// registry, and applies the plan through `SchemaService`. Components are synced
/// first (a component field references a component), then content types.
/// Fail-fast: the first error aborts (and propagates so the server refuses to boot).
pub async fn sync_from_path(
    schemas: &crate::SchemaService,
    components: &crate::ComponentService,
    path: &str,
    mode: SyncMode,
) -> Result<(), Error> {
    let path = Path::new(path);
    let desired = load_desired(path)?;

    // ---- components first (a component field references a component) ----
    let cur_components = components.registry().list().await;
    let comp_actions = plan_components(&desired.components, &cur_components, mode)?;
    apply_components(components, schemas, &desired, comp_actions).await?;

    // ---- then content types (existing logic) ----
    for ct in &desired.content_types {
        ct.validate().map_err(Error::from)?;
    }

    let current = schemas.registry().list().await;
    let actions = plan_sync(&desired.content_types, &current, mode)?;

    let (creates, others): (Vec<_>, Vec<_>) = actions
        .into_iter()
        .partition(|a| matches!(a, SyncAction::Create(_)));
    let create_cts: Vec<NewContentType> = creates
        .into_iter()
        .map(|a| match a {
            SyncAction::Create(c) => c,
            _ => unreachable!(),
        })
        .collect();

    let mut created = 0usize;
    let mut patched = 0usize;
    let mut dropped = 0usize;
    let mut unmanaged = 0usize;

    for mut nct in order_creates(create_cts) {
        nct.options = managed_options(&nct.options);
        schemas.create(nct).await?;
        created += 1;
    }

    // Collect drop names first so they can be ordered before execution.
    let mut drop_names: Vec<String> = Vec::new();
    let mut patch_actions: Vec<SyncAction> = Vec::new();
    let mut unmanage_actions: Vec<SyncAction> = Vec::new();

    for action in others {
        match action {
            SyncAction::DropType(name) => drop_names.push(name),
            SyncAction::Patch { .. } => patch_actions.push(action),
            SyncAction::Unmanage(_) => unmanage_actions.push(action),
            SyncAction::Create(_) => unreachable!("creates handled above"),
        }
    }

    for action in patch_actions {
        match action {
            SyncAction::Patch {
                name,
                add_fields,
                drop_fields,
                options,
            } => {
                let existing = schemas.registry().get(&name).await;
                let options_changed = existing
                    .as_ref()
                    .map(|e| e.options != options)
                    .unwrap_or(true);
                if add_fields.is_empty() && drop_fields.is_empty() && !options_changed {
                    continue;
                }
                let patch = rustapi_core::PatchContentType {
                    display_name: None,
                    add_fields,
                    drop_fields,
                    extend_enum_values: vec![],
                    options: Some(options),
                };
                schemas.patch(&name, patch).await?;
                patched += 1;
            }
            _ => unreachable!(),
        }
    }

    for name in order_drops(drop_names, &current) {
        schemas.delete(&name).await?;
        dropped += 1;
    }

    for action in unmanage_actions {
        match action {
            SyncAction::Unmanage(name) => {
                if let Some(existing) = schemas.registry().get(&name).await {
                    let mut obj = existing.options.as_object().cloned().unwrap_or_default();
                    obj.remove("managed");
                    let patch = rustapi_core::PatchContentType {
                        display_name: None,
                        add_fields: vec![],
                        drop_fields: vec![],
                        extend_enum_values: vec![],
                        options: Some(serde_json::Value::Object(obj)),
                    };
                    schemas.patch(&name, patch).await?;
                    unmanaged += 1;
                }
            }
            _ => unreachable!(),
        }
    }

    tracing::info!(
        created,
        patched,
        dropped,
        unmanaged,
        ?mode,
        "schema sync complete"
    );
    Ok(())
}

/// Apply component actions: create/update with managed=true, delete (full) or
/// unmanage (additive). Delete is blocked by ComponentService when the component
/// is still referenced by a content type — surfaces as a fail-fast error.
async fn apply_components(
    components: &crate::ComponentService,
    schemas: &crate::SchemaService,
    desired: &ParsedSchema,
    actions: Vec<ComponentAction>,
) -> Result<(), Error> {
    for action in actions {
        match action {
            ComponentAction::Create(c) => {
                components
                    .create(&c.uid, &c.display_name, c.fields, true)
                    .await?;
            }
            ComponentAction::Update(c) => {
                components
                    .update(&c.uid, &c.display_name, c.fields, true)
                    .await?;
            }
            ComponentAction::Delete(uid) => {
                let referencing = referencing_types(&uid, desired, schemas).await;
                components.delete(&uid, &referencing).await?;
            }
            ComponentAction::Unmanage(uid) => {
                if let Some(existing) = components.registry().get(&uid).await {
                    components
                        .update(
                            &existing.uid,
                            &existing.display_name,
                            existing.fields,
                            false,
                        )
                        .await?;
                }
            }
        }
    }
    Ok(())
}

/// Content-type names that reference component `uid`, from both desired TOML types
/// and the live registry (a superset → conservative; the service check is backstop).
async fn referencing_types(
    uid: &str,
    desired: &ParsedSchema,
    schemas: &crate::SchemaService,
) -> Vec<String> {
    use std::collections::HashSet;
    let refs = |fields: &[Field]| {
        fields.iter().any(|f| {
            f.component_meta()
                .map(|m| m.component == uid)
                .unwrap_or(false)
        })
    };
    let mut names: HashSet<String> = HashSet::new();
    for ct in &desired.content_types {
        if refs(&ct.fields) {
            names.insert(ct.name.clone());
        }
    }
    for ct in schemas.registry().list().await {
        if refs(&ct.fields) {
            names.insert(ct.name.clone());
        }
    }
    names.into_iter().collect()
}

use rustapi_sql::Component;

/// One reconciliation step for components.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentAction {
    Create(TomlComponent),
    Update(TomlComponent),
    Delete(String),
    Unmanage(String),
}

/// Pure diff for components. `desired` is the TOML set, `current` the live
/// component registry list.
pub(crate) fn plan_components(
    desired: &[TomlComponent],
    current: &[Component],
    mode: SyncMode,
) -> Result<Vec<ComponentAction>, Error> {
    use std::collections::HashMap;
    let cur: HashMap<&str, &Component> = current.iter().map(|c| (c.uid.as_str(), c)).collect();
    let des: HashMap<&str, &TomlComponent> = desired.iter().map(|c| (c.uid.as_str(), c)).collect();
    let mut actions = Vec::new();
    for d in desired {
        match cur.get(d.uid.as_str()) {
            None => actions.push(ComponentAction::Create(d.clone())),
            Some(existing) => {
                if existing.display_name != d.display_name || existing.fields != d.fields {
                    actions.push(ComponentAction::Update(d.clone()));
                }
            }
        }
    }
    for c in current {
        if !des.contains_key(c.uid.as_str()) {
            match mode {
                SyncMode::Full => actions.push(ComponentAction::Delete(c.uid.clone())),
                SyncMode::Additive => {
                    if c.managed {
                        actions.push(ComponentAction::Unmanage(c.uid.clone()));
                    }
                }
            }
        }
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::FieldKind;
    use serde_json::json;
    use uuid::Uuid;

    fn fld(name: &str) -> Field {
        Field {
            name: name.into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        }
    }

    fn nct(name: &str, fields: Vec<Field>) -> NewContentType {
        NewContentType {
            name: name.into(),
            display_name: name.into(),
            fields,
            options: json!({}),
            kind: rustapi_core::ContentTypeKind::Collection,
        }
    }

    fn ct(name: &str, fields: Vec<Field>, managed: bool) -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: name.into(),
            display_name: name.into(),
            fields,
            options: if managed {
                json!({ "managed": true })
            } else {
                json!({})
            },
            kind: rustapi_core::ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn comp(uid: &str, fields: Vec<Field>, managed: bool) -> rustapi_sql::Component {
        rustapi_sql::Component {
            uid: uid.into(),
            display_name: uid.into(),
            fields,
            managed,
        }
    }
    fn tcomp(uid: &str, fields: Vec<Field>) -> super::TomlComponent {
        super::TomlComponent {
            uid: uid.into(),
            display_name: uid.into(),
            fields,
        }
    }

    #[test]
    fn plan_components_create_update_skip() {
        let desired = vec![tcomp("shared.seo", vec![fld("title")])];
        let acts = plan_components(&desired, &[], SyncMode::Additive).unwrap();
        assert!(matches!(&acts[0], ComponentAction::Create(c) if c.uid == "shared.seo"));

        let cur = vec![comp("shared.seo", vec![fld("title")], true)];
        let acts = plan_components(&desired, &cur, SyncMode::Additive).unwrap();
        assert!(acts.is_empty(), "equal component must produce no action");

        let desired2 = vec![tcomp("shared.seo", vec![fld("title"), fld("body")])];
        let acts = plan_components(&desired2, &cur, SyncMode::Additive).unwrap();
        assert!(matches!(&acts[0], ComponentAction::Update(c) if c.fields.len() == 2));
    }

    #[test]
    fn plan_components_delete_full_unmanage_additive() {
        let cur = vec![comp("shared.seo", vec![fld("title")], true)];
        let full = plan_components(&[], &cur, SyncMode::Full).unwrap();
        assert_eq!(full, vec![ComponentAction::Delete("shared.seo".into())]);
        let add = plan_components(&[], &cur, SyncMode::Additive).unwrap();
        assert_eq!(add, vec![ComponentAction::Unmanage("shared.seo".into())]);
    }

    #[test]
    fn plan_components_unmanaged_db_only_left_alone_additive() {
        let cur = vec![comp("ui.only", vec![fld("title")], false)];
        let add = plan_components(&[], &cur, SyncMode::Additive).unwrap();
        assert!(add.is_empty());
    }

    #[test]
    fn parses_component_blocks() {
        let doc = r#"
[[component]]
uid = "shared.seo"
display_name = "SEO"
  [[component.field]]
  name = "meta_title"
  kind = "string"

[[content_type]]
name = "post"
display_name = "Post"
  [[content_type.field]]
  name = "title"
  kind = "string"
"#;
        let parsed = parse_schema(doc).expect("parse");
        assert_eq!(parsed.content_types.len(), 1);
        assert_eq!(parsed.components.len(), 1);
        assert_eq!(parsed.components[0].uid, "shared.seo");
        assert_eq!(parsed.components[0].fields[0].name, "meta_title");
    }

    #[test]
    fn diff_creates_missing_type() {
        let desired = vec![nct("post", vec![fld("title")])];
        let actions = plan_sync(&desired, &[], SyncMode::Additive).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], SyncAction::Create(c) if c.name == "post"));
    }

    #[test]
    fn diff_adds_missing_field() {
        let desired = vec![nct("post", vec![fld("title"), fld("body")])];
        let current = vec![ct("post", vec![fld("title")], true)];
        let actions = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &actions[0] {
            SyncAction::Patch {
                add_fields,
                drop_fields,
                ..
            } => {
                assert_eq!(add_fields.len(), 1);
                assert_eq!(add_fields[0].name, "body");
                assert!(drop_fields.is_empty());
            }
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn diff_drop_field_only_in_full() {
        let desired = vec![nct("post", vec![fld("title")])];
        let current = vec![ct("post", vec![fld("title"), fld("body")], true)];

        let add = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &add[0] {
            SyncAction::Patch { drop_fields, .. } => assert!(drop_fields.is_empty()),
            other => panic!("expected Patch, got {other:?}"),
        }

        let full = plan_sync(&desired, &current, SyncMode::Full).unwrap();
        match &full[0] {
            SyncAction::Patch { drop_fields, .. } => {
                assert_eq!(drop_fields, &vec!["body".to_string()])
            }
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn diff_drop_type_full_unmanage_additive() {
        let current = vec![ct("legacy", vec![fld("x")], true)];
        let full = plan_sync(&[], &current, SyncMode::Full).unwrap();
        assert_eq!(full, vec![SyncAction::DropType("legacy".into())]);

        let add = plan_sync(&[], &current, SyncMode::Additive).unwrap();
        assert_eq!(add, vec![SyncAction::Unmanage("legacy".into())]);
    }

    #[test]
    fn diff_unmanaged_db_only_type_left_alone_additive() {
        let current = vec![ct("uionly", vec![fld("x")], false)];
        let add = plan_sync(&[], &current, SyncMode::Additive).unwrap();
        assert!(add.is_empty());
    }

    #[test]
    fn diff_field_kind_change_errors() {
        let mut changed = fld("title");
        changed.kind = FieldKind::Integer;
        let desired = vec![nct("post", vec![changed])];
        let current = vec![ct("post", vec![fld("title")], true)];
        let err = plan_sync(&desired, &current, SyncMode::Full).unwrap_err();
        assert!(format!("{err:?}").contains("changed kind/meta"));
    }

    #[test]
    fn diff_patch_sets_managed_option() {
        let desired = vec![nct("post", vec![fld("title")])];
        let current = vec![ct("post", vec![fld("title")], false)];
        let actions = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &actions[0] {
            SyncAction::Patch { options, .. } => {
                assert_eq!(options.get("managed").and_then(|v| v.as_bool()), Some(true));
            }
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn parses_type_with_fields() {
        let doc = r#"
[[content_type]]
name = "post"
display_name = "Post"
kind = "collection"
options = { draft_publish = true }

  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true
"#;
        let cts = parse_schema(doc).expect("parse").content_types;
        assert_eq!(cts.len(), 1);
        assert_eq!(cts[0].name, "post");
        assert_eq!(cts[0].fields.len(), 1);
        assert_eq!(cts[0].fields[0].name, "title");
        assert_eq!(cts[0].fields[0].kind, FieldKind::String);
        assert!(cts[0].fields[0].required);
    }

    #[test]
    fn sync_mode_from_env() {
        assert_eq!(SyncMode::from_env_str("full"), SyncMode::Full);
        assert_eq!(SyncMode::from_env_str("FULL"), SyncMode::Full);
        assert_eq!(SyncMode::from_env_str("additive"), SyncMode::Additive);
        assert_eq!(SyncMode::from_env_str("garbage"), SyncMode::Additive);
    }

    #[test]
    fn blog_preset_parses_and_orders() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/schema/blog");
        let desired = super::load_desired(&dir).expect("load blog preset");
        let names: Vec<&str> = desired
            .content_types
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(names.contains(&"author"));
        assert!(names.contains(&"post"));
        for c in &desired.content_types {
            c.validate().expect("preset type valid");
        }
        assert!(
            desired.components.iter().any(|c| c.uid == "shared.seo"),
            "blog preset must include shared.seo component"
        );
        let ordered = super::order_creates(desired.content_types);
        let pos = |n: &str| ordered.iter().position(|c| c.name == n).unwrap();
        assert!(
            pos("author") < pos("post"),
            "author must be created before post"
        );
    }

    #[test]
    fn order_drops_removes_dependent_before_target() {
        // author (no rels) + post (relation -> author). Both dropped in full mode.
        // post must be deleted before author.
        let author = ct("author", vec![fld("name")], true);
        let mut rel = fld("author");
        rel.kind = FieldKind::Relation;
        rel.kind_meta = json!({"target": "author", "cardinality": "many_to_one"});
        let post = ct("post", vec![fld("title"), rel], true);
        let ordered = super::order_drops(
            vec!["author".to_string(), "post".to_string()],
            &[author, post],
        );
        let pos = |n: &str| ordered.iter().position(|x| x == n).unwrap();
        assert!(
            pos("post") < pos("author"),
            "post (dependent) must drop before author (target)"
        );
    }

    #[test]
    fn diff_idempotent_when_options_match_resolved() {
        // Existing type already stored with resolved+managed options. A re-plan must
        // produce a Patch whose options EQUAL the stored options (so apply skips it).
        let desired = vec![nct("post", vec![fld("title")])];
        // Simulate stored state after first sync: resolved_options + managed.
        let stored_opts = serde_json::json!({ "draft_publish": false, "managed": true });
        let mut existing = ct("post", vec![fld("title")], true);
        existing.options = stored_opts.clone();
        let actions = plan_sync(&desired, &[existing], SyncMode::Additive).unwrap();
        match &actions[0] {
            SyncAction::Patch {
                options,
                add_fields,
                drop_fields,
                ..
            } => {
                assert!(add_fields.is_empty() && drop_fields.is_empty());
                assert_eq!(
                    options, &stored_opts,
                    "planned options must equal stored so apply is a no-op"
                );
            }
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn order_creates_places_target_before_dependent() {
        let mut post = nct("post", vec![fld("title")]);
        let mut rel = fld("author");
        rel.kind = FieldKind::Relation;
        rel.kind_meta = json!({"target": "author", "cardinality": "many_to_one"});
        post.fields.push(rel);
        let author = nct("author", vec![fld("name")]);

        // Declared post-first; ordering must move author ahead.
        let ordered = super::order_creates(vec![post, author]);
        assert_eq!(ordered[0].name, "author");
        assert_eq!(ordered[1].name, "post");
    }
}
