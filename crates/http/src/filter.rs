//! Strapi-style `?filters[col][$op]=value` parser. Produces a `rustapi_sql::Filter`
//! ready for the SQL builder. v1 supports `$eq`, `$ne`, `$null` with implicit
//! AND across params.

use rustapi_core::{is_system_column, BoundValue, ContentType, Error, Field, FieldKind, ValidationErrors, SYSTEM_COLUMNS};
use rustapi_sql::{Condition, Filter, FilterValue, Op};
use url::form_urlencoded;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment {
    /// `$or`, `$and`, `$not`
    Combinator(String),
    /// `$eq`, `$ne`, `$null`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin`,
    /// `$contains`, `$startsWith`, `$endsWith`, `$containsi`
    Op(String),
    /// Group child index (`$or[0]`, `$and[2]`) or set-value index (`$in[3]`).
    Index(usize),
    /// Column name.
    Name(String),
}

/// Split a `filters[...]...` key into ordered `Segment`s. Performs no
/// semantic validation — that's the tree builder's job.
pub(crate) fn tokenize_key(k: &str) -> Result<Vec<Segment>, Error> {
    let rest = k.strip_prefix("filters").ok_or_else(|| {
        Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}` (missing `filters` prefix)"
        )))
    })?;

    let mut segments = Vec::new();
    let mut cur = rest;
    while !cur.is_empty() {
        let inner = cur
            .strip_prefix('[')
            .and_then(|s| {
                let close = s.find(']')?;
                Some((&s[..close], &s[close + 1..]))
            })
            .ok_or_else(|| {
                Error::Validation(ValidationErrors::single(format!(
                    "malformed filter param `{k}` (unbalanced brackets)"
                )))
            })?;
        let (raw, tail) = inner;
        if raw.is_empty() {
            return Err(Error::Validation(ValidationErrors::single(format!(
                "malformed filter param `{k}` (empty bracket)"
            ))));
        }
        let seg = classify_segment(raw);
        segments.push(seg);
        cur = tail;
    }
    if segments.is_empty() {
        return Err(Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}` (no segments)"
        ))));
    }
    Ok(segments)
}

fn classify_segment(raw: &str) -> Segment {
    match raw {
        "$or" | "$and" | "$not" => Segment::Combinator(raw.to_string()),
        s if s.starts_with('$') => Segment::Op(s.to_string()),
        s => match s.parse::<usize>() {
            Ok(n) => Segment::Index(n),
            Err(_) => Segment::Name(s.to_string()),
        },
    }
}

/// Parse a raw query string into a `Filter`. Non-filter params are ignored.
/// Returns `Filter::None` if no filter params are present.
pub fn parse(raw_query: &str, ct: &ContentType) -> Result<Filter, Error> {
    const MAX_DEPTH: usize = 8;
    const MAX_LEAVES: usize = 100;

    let mut root = TreeNode::group_all();
    let mut set_buckets: SetBuckets = std::collections::HashMap::new();
    let mut leaf_count: usize = 0;

    for (k, v) in form_urlencoded::parse(raw_query.as_bytes()) {
        if !k.starts_with("filters[") && k != "filters" {
            continue;
        }
        let segs = tokenize_key(&k)?;
        insert_segments(
            &mut root,
            &segs,
            &v,
            ct,
            &mut set_buckets,
            &mut leaf_count,
            MAX_DEPTH,
            MAX_LEAVES,
        )?;
    }

    flush_set_buckets(&mut root, set_buckets, ct)?;
    finalize(root)
}

type SetBuckets = std::collections::HashMap<SetKey, SetBucket>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PathStep {
    /// Step into parent_map[slot_idx] (a GroupAll or GroupAny), then into its
    /// inner map at `child_idx`.
    Group { slot_idx: usize, child_idx: usize },
    /// Step into parent_map[slot_idx] (a Not), then into the singleton
    /// holder's index 0.
    Not { slot_idx: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SetKey {
    /// Steps from root to the leaf's parent.
    path: Vec<PathStep>,
    column: String,
    op: Op,
}

struct SetBucket {
    kind: FieldKind,
    /// BTreeMap so we walk in index order and detect gaps cheaply.
    values: std::collections::BTreeMap<usize, BoundValue>,
}

/// Mutable in-progress tree. Leaves are produced directly; group ordering
/// is preserved via BTreeMap on indices.
#[derive(Debug)]
enum TreeNode {
    Leaf(Condition),
    GroupAll(std::collections::BTreeMap<usize, TreeNode>),
    GroupAny(std::collections::BTreeMap<usize, TreeNode>),
    Not(Box<Option<TreeNode>>),
}

impl TreeNode {
    fn group_all() -> Self {
        TreeNode::GroupAll(std::collections::BTreeMap::new())
    }
    fn group_any() -> Self {
        TreeNode::GroupAny(std::collections::BTreeMap::new())
    }
    fn not_empty() -> Self {
        TreeNode::Not(Box::new(None))
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_segments(
    root: &mut TreeNode,
    segs: &[Segment],
    raw_val: &str,
    ct: &ContentType,
    set_buckets: &mut SetBuckets,
    leaf_count: &mut usize,
    max_depth: usize,
    max_leaves: usize,
) -> Result<(), Error> {
    insert_into(
        root,
        segs,
        &mut Vec::new(),
        raw_val,
        ct,
        set_buckets,
        0,
        leaf_count,
        max_depth,
        max_leaves,
    )
}

/// Walk `segs` into `parent`. Combinators at the head of `segs` reuse an
/// existing matching-tag sibling under `parent` so two query params for the
/// same group stack into the same node; otherwise a fresh trailing slot is
/// allocated. Leaves claim a fresh trailing slot and reject duplicate
/// `(column, op)` pairs among sibling leaves at the same level.
#[allow(clippy::too_many_arguments)]
fn insert_into(
    parent: &mut TreeNode,
    segs: &[Segment],
    path: &mut Vec<PathStep>,
    raw_val: &str,
    ct: &ContentType,
    set_buckets: &mut SetBuckets,
    depth: usize,
    leaf_count: &mut usize,
    max_depth: usize,
    max_leaves: usize,
) -> Result<(), Error> {
    if depth > max_depth {
        return Err(generic_err("filter nesting depth exceeds 8"));
    }
    match segs.first() {
        Some(Segment::Combinator(tag)) if tag == "$not" => {
            let inner_segs = &segs[1..];
            if inner_segs.is_empty() {
                return Err(generic_err("$not requires a child"));
            }
            if matches!(inner_segs.first(), Some(Segment::Index(_))) {
                return Err(generic_err("$not must be unary"));
            }
            let parent_map = parent_group_map_mut(parent)?;
            let fallback = parent_map.len();
            let slot_idx = find_existing_combinator(parent_map, "$not").unwrap_or(fallback);
            let entry = parent_map
                .entry(slot_idx)
                .or_insert_with(TreeNode::not_empty);
            let TreeNode::Not(slot) = entry else {
                return Err(generic_err("$not collides with non-$not child at same index"));
            };
            if slot.is_none() {
                **slot = Some(TreeNode::group_all());
            }
            let holder = slot
                .as_mut()
                .as_mut()
                .expect("just initialized");
            path.push(PathStep::Not { slot_idx });
            let res = insert_into(
                holder,
                inner_segs,
                path,
                raw_val,
                ct,
                set_buckets,
                depth + 1,
                leaf_count,
                max_depth,
                max_leaves,
            );
            path.pop();
            res
        }
        Some(Segment::Combinator(tag)) => {
            let group_tag = tag.clone();
            let idx_seg = segs.get(1);
            let Some(Segment::Index(child_idx)) = idx_seg else {
                return Err(generic_err(&format!(
                    "{group_tag} group requires bracketed index next"
                )));
            };
            let parent_map = parent_group_map_mut(parent)?;
            let fallback = parent_map.len();
            let slot_idx =
                find_existing_combinator(parent_map, &group_tag).unwrap_or(fallback);
            let group_entry = parent_map
                .entry(slot_idx)
                .or_insert_with(|| if group_tag == "$or" {
                    TreeNode::group_any()
                } else {
                    TreeNode::group_all()
                });
            ensure_matches_combinator(group_entry, &group_tag)?;
            path.push(PathStep::Group { slot_idx, child_idx: *child_idx });
            let remainder = &segs[2..];
            let res = insert_at_group_index(
                group_entry,
                *child_idx,
                remainder,
                path,
                raw_val,
                ct,
                set_buckets,
                depth + 1,
                leaf_count,
                max_depth,
                max_leaves,
            );
            path.pop();
            res
        }
        Some(Segment::Name(col)) => {
            let op_seg = segs.get(1).ok_or_else(|| field_err(col, "missing operator"))?;
            let Segment::Op(op_str) = op_seg else {
                return Err(field_err(col, "expected operator after column"));
            };
            let op = map_op(op_str, col)?;
            let field = field_for(ct, col)?;
            let kind = field.kind();
            if !rustapi_sql::op_allows_kind(op, kind) {
                return Err(field_err(
                    col,
                    format!("operator `{op_str}` invalid for kind `{kind:?}`"),
                ));
            }
            let extra = segs.get(2);
            let is_set_op = matches!(op, Op::In | Op::NotIn);
            match (is_set_op, extra) {
                (true, Some(Segment::Index(i))) => {
                    if raw_val.eq_ignore_ascii_case("null") {
                        return Err(field_err(col, "set operator entries cannot be null"));
                    }
                    let bv = coerce_bound(kind, col, raw_val)?;
                    let key = SetKey {
                        path: path.clone(),
                        column: col.clone(),
                        op,
                    };
                    let is_new_bucket = !set_buckets.contains_key(&key);
                    let bucket = set_buckets
                        .entry(key)
                        .or_insert_with(|| SetBucket {
                            kind,
                            values: std::collections::BTreeMap::new(),
                        });
                    if bucket.values.insert(*i, bv).is_some() {
                        return Err(field_err(col, "duplicate set operator entry"));
                    }
                    if bucket.values.len() > 100 {
                        return Err(field_err(col, "set operator limited to 100 items"));
                    }
                    if is_new_bucket {
                        *leaf_count += 1;
                        if *leaf_count > max_leaves {
                            return Err(generic_err("filter leaf count exceeds 100"));
                        }
                    }
                    Ok(())
                }
                (true, _) => Err(field_err(col, "set operator requires bracketed list indices")),
                (false, Some(_)) => Err(field_err(col, "unexpected list index for operator")),
                (false, None) => {
                    let parent_map = parent_group_map_mut(parent)?;
                    if has_sibling_leaf(parent_map, col, op) {
                        return Err(field_err(col, "duplicate filter operator on column"));
                    }
                    let value = coerce_value(field, op, col, raw_val)?;
                    let next_idx = parent_map.len();
                    parent_map.insert(
                        next_idx,
                        TreeNode::Leaf(Condition::new(col, kind, op, value)),
                    );
                    *leaf_count += 1;
                    if *leaf_count > max_leaves {
                        return Err(generic_err("filter leaf count exceeds 100"));
                    }
                    Ok(())
                }
            }
        }
        Some(Segment::Op(_) | Segment::Index(_)) | None => {
            Err(generic_err("malformed filter key shape"))
        }
    }
}

/// Place `segs` at the given `child_idx` inside an already-resolved group
/// `parent`. If `segs` is a leaf (`Name`/`Op`/maybe `Index`), the leaf
/// occupies that slot directly; if it's a nested combinator, the slot is
/// (re)used as that sub-combinator's node.
#[allow(clippy::too_many_arguments)]
fn insert_at_group_index(
    parent: &mut TreeNode,
    child_idx: usize,
    segs: &[Segment],
    path: &mut Vec<PathStep>,
    raw_val: &str,
    ct: &ContentType,
    set_buckets: &mut SetBuckets,
    depth: usize,
    leaf_count: &mut usize,
    max_depth: usize,
    max_leaves: usize,
) -> Result<(), Error> {
    if depth > max_depth {
        return Err(generic_err("filter nesting depth exceeds 8"));
    }
    match segs.first() {
        Some(Segment::Name(col)) => {
            let op_seg = segs.get(1).ok_or_else(|| field_err(col, "missing operator"))?;
            let Segment::Op(op_str) = op_seg else {
                return Err(field_err(col, "expected operator after column"));
            };
            let op = map_op(op_str, col)?;
            let field = field_for(ct, col)?;
            let kind = field.kind();
            if !rustapi_sql::op_allows_kind(op, kind) {
                return Err(field_err(
                    col,
                    format!("operator `{op_str}` invalid for kind `{kind:?}`"),
                ));
            }
            let extra = segs.get(2);
            let is_set_op = matches!(op, Op::In | Op::NotIn);
            match (is_set_op, extra) {
                (true, Some(Segment::Index(i))) => {
                    if raw_val.eq_ignore_ascii_case("null") {
                        return Err(field_err(col, "set operator entries cannot be null"));
                    }
                    let bv = coerce_bound(kind, col, raw_val)?;
                    let key = SetKey {
                        path: path.clone(),
                        column: col.clone(),
                        op,
                    };
                    let is_new_bucket = !set_buckets.contains_key(&key);
                    let bucket = set_buckets
                        .entry(key)
                        .or_insert_with(|| SetBucket {
                            kind,
                            values: std::collections::BTreeMap::new(),
                        });
                    if bucket.values.insert(*i, bv).is_some() {
                        return Err(field_err(col, "duplicate set operator entry"));
                    }
                    if bucket.values.len() > 100 {
                        return Err(field_err(col, "set operator limited to 100 items"));
                    }
                    if is_new_bucket {
                        *leaf_count += 1;
                        if *leaf_count > max_leaves {
                            return Err(generic_err("filter leaf count exceeds 100"));
                        }
                    }
                    Ok(())
                }
                (true, _) => Err(field_err(col, "set operator requires bracketed list indices")),
                (false, Some(_)) => Err(field_err(col, "unexpected list index for operator")),
                (false, None) => {
                    let parent_map = parent_group_map_mut(parent)?;
                    if parent_map.contains_key(&child_idx) {
                        return Err(generic_err("duplicate filter at same path"));
                    }
                    let value = coerce_value(field, op, col, raw_val)?;
                    parent_map.insert(
                        child_idx,
                        TreeNode::Leaf(Condition::new(col, kind, op, value)),
                    );
                    *leaf_count += 1;
                    if *leaf_count > max_leaves {
                        return Err(generic_err("filter leaf count exceeds 100"));
                    }
                    Ok(())
                }
            }
        }
        Some(Segment::Combinator(tag)) if tag == "$not" => {
            let inner_segs = &segs[1..];
            if inner_segs.is_empty() {
                return Err(generic_err("$not requires a child"));
            }
            if matches!(inner_segs.first(), Some(Segment::Index(_))) {
                return Err(generic_err("$not must be unary"));
            }
            let parent_map = parent_group_map_mut(parent)?;
            let entry = parent_map
                .entry(child_idx)
                .or_insert_with(TreeNode::not_empty);
            let TreeNode::Not(slot) = entry else {
                return Err(generic_err("$not collides with non-$not child at same index"));
            };
            if slot.is_none() {
                **slot = Some(TreeNode::group_all());
            }
            let holder = slot.as_mut().as_mut().expect("just initialized");
            path.push(PathStep::Not { slot_idx: child_idx });
            let res = insert_into(
                holder,
                inner_segs,
                path,
                raw_val,
                ct,
                set_buckets,
                depth + 1,
                leaf_count,
                max_depth,
                max_leaves,
            );
            path.pop();
            res
        }
        Some(Segment::Combinator(tag)) => {
            let group_tag = tag.clone();
            let idx_seg = segs.get(1);
            let Some(Segment::Index(grand_idx)) = idx_seg else {
                return Err(generic_err(&format!(
                    "{group_tag} group requires bracketed index next"
                )));
            };
            let parent_map = parent_group_map_mut(parent)?;
            let entry = parent_map.entry(child_idx).or_insert_with(|| {
                if group_tag == "$or" {
                    TreeNode::group_any()
                } else {
                    TreeNode::group_all()
                }
            });
            ensure_matches_combinator(entry, &group_tag)?;
            path.push(PathStep::Group { slot_idx: child_idx, child_idx: *grand_idx });
            let remainder = &segs[2..];
            let res = insert_at_group_index(
                entry,
                *grand_idx,
                remainder,
                path,
                raw_val,
                ct,
                set_buckets,
                depth + 1,
                leaf_count,
                max_depth,
                max_leaves,
            );
            path.pop();
            res
        }
        Some(Segment::Op(_) | Segment::Index(_)) | None => {
            Err(generic_err("malformed filter key shape"))
        }
    }
}

fn find_existing_combinator(
    map: &std::collections::BTreeMap<usize, TreeNode>,
    tag: &str,
) -> Option<usize> {
    for (idx, node) in map {
        let is_match = matches!(
            (tag, node),
            ("$or", TreeNode::GroupAny(_))
                | ("$and", TreeNode::GroupAll(_))
                | ("$not", TreeNode::Not(_))
        );
        if is_match {
            return Some(*idx);
        }
    }
    None
}

fn has_sibling_leaf(
    map: &std::collections::BTreeMap<usize, TreeNode>,
    col: &str,
    op: Op,
) -> bool {
    map.values()
        .any(|n| matches!(n, TreeNode::Leaf(c) if c.column == col && c.op == op))
}

fn parent_group_map_mut(
    parent: &mut TreeNode,
) -> Result<&mut std::collections::BTreeMap<usize, TreeNode>, Error> {
    match parent {
        TreeNode::GroupAll(m) | TreeNode::GroupAny(m) => Ok(m),
        _ => Err(generic_err("internal: cannot insert into non-group parent")),
    }
}

fn ensure_matches_combinator(node: &TreeNode, tag: &str) -> Result<(), Error> {
    let ok = matches!(
        (tag, node),
        ("$or", TreeNode::GroupAny(_)) | ("$and", TreeNode::GroupAll(_))
    );
    if ok {
        Ok(())
    } else {
        Err(generic_err(&format!(
            "combinator `{tag}` collides with existing node at the same path"
        )))
    }
}

fn flush_set_buckets(
    root: &mut TreeNode,
    set_buckets: SetBuckets,
    _ct: &ContentType,
) -> Result<(), Error> {
    for (key, bucket) in set_buckets {
        if bucket.values.is_empty() {
            return Err(field_err(&key.column, "set operator requires non-empty list"));
        }
        for (expected, actual) in bucket.values.keys().enumerate() {
            if expected != *actual {
                return Err(field_err(&key.column, "gap in set operator indices"));
            }
        }
        let values: Vec<BoundValue> = bucket.values.into_values().collect();
        let cond = Condition::new(
            key.column.clone(),
            bucket.kind,
            key.op,
            FilterValue::List(values),
        );
        insert_at_path(root, &key.path, cond)?;
    }
    Ok(())
}

fn insert_at_path(
    root: &mut TreeNode,
    path: &[PathStep],
    cond: Condition,
) -> Result<(), Error> {
    // Top-level (no combinator wrapping): allocate a fresh trailing slot at root.
    if path.is_empty() {
        let map = parent_group_map_mut(root)?;
        if has_sibling_leaf(map, &cond.column, cond.op) {
            return Err(field_err(&cond.column, "duplicate filter operator on column"));
        }
        let next = map.len();
        map.insert(next, TreeNode::Leaf(cond));
        return Ok(());
    }

    // Walk to the leaf's immediate parent group, recording the final
    // child_idx so we can write the leaf directly into that slot.
    let mut node: &mut TreeNode = root;
    let last = path.len() - 1;
    let mut final_child_idx: Option<usize> = None;

    for (i, step) in path.iter().enumerate() {
        let is_last = i == last;
        match step {
            PathStep::Group { slot_idx, child_idx } => {
                let map = parent_group_map_mut(node)?;
                node = map.get_mut(slot_idx).ok_or_else(|| {
                    generic_err("internal: set-bucket path missing during flush")
                })?;
                if is_last {
                    final_child_idx = Some(*child_idx);
                } else {
                    let inner_map = parent_group_map_mut(node)?;
                    node = inner_map.get_mut(child_idx).ok_or_else(|| {
                        generic_err("internal: set-bucket path missing during flush")
                    })?;
                }
            }
            PathStep::Not { slot_idx } => {
                let map = parent_group_map_mut(node)?;
                node = map.get_mut(slot_idx).ok_or_else(|| {
                    generic_err("internal: set-bucket path missing during flush")
                })?;
                let TreeNode::Not(slot) = node else {
                    return Err(generic_err("internal: expected Not at $not path step"));
                };
                node = slot.as_mut().as_mut().ok_or_else(|| {
                    generic_err("internal: $not slot empty during flush")
                })?;
                if is_last {
                    final_child_idx = Some(0);
                } else {
                    let inner_map = parent_group_map_mut(node)?;
                    node = inner_map.get_mut(&0).ok_or_else(|| {
                        generic_err("internal: $not holder empty during flush")
                    })?;
                }
            }
        }
    }

    let map = parent_group_map_mut(node)?;
    let slot = final_child_idx.expect("path was non-empty");
    if map.contains_key(&slot) {
        return Err(generic_err("duplicate filter at same path"));
    }
    map.insert(slot, TreeNode::Leaf(cond));
    Ok(())
}

fn finalize(root: TreeNode) -> Result<Filter, Error> {
    let TreeNode::GroupAll(map) = root else {
        return Err(generic_err("internal: root is not All"));
    };
    if map.is_empty() {
        return Ok(Filter::None);
    }
    for (expected, actual) in map.keys().enumerate() {
        if expected != *actual {
            return Err(generic_err("gap in top-level filter ordering"));
        }
    }
    let children: Vec<Filter> = map.into_values().map(node_to_filter).collect::<Result<_, _>>()?;
    Ok(Filter::All(children))
}

fn node_to_filter(node: TreeNode) -> Result<Filter, Error> {
    match node {
        TreeNode::Leaf(c) => Ok(Filter::Leaf(c)),
        TreeNode::GroupAll(map) => {
            if map.is_empty() {
                return Err(generic_err("empty $and group"));
            }
            for (expected, actual) in map.keys().enumerate() {
                if expected != *actual {
                    return Err(generic_err("gap in $and group indices"));
                }
            }
            let xs: Vec<Filter> = map.into_values().map(node_to_filter).collect::<Result<_, _>>()?;
            Ok(Filter::All(xs))
        }
        TreeNode::GroupAny(map) => {
            if map.is_empty() {
                return Err(generic_err("empty $or group"));
            }
            for (expected, actual) in map.keys().enumerate() {
                if expected != *actual {
                    return Err(generic_err("gap in $or group indices"));
                }
            }
            let xs: Vec<Filter> = map.into_values().map(node_to_filter).collect::<Result<_, _>>()?;
            Ok(Filter::Any(xs))
        }
        TreeNode::Not(slot) => {
            let inner = slot.ok_or_else(|| generic_err("$not requires a child"))?;
            let unwrapped = match inner {
                TreeNode::GroupAll(mut m) if m.len() == 1 => {
                    let key = *m.keys().next().expect("len==1 just checked");
                    m.remove(&key).expect("key just observed")
                }
                TreeNode::GroupAll(_) => {
                    return Err(generic_err("$not holder must have exactly one child"));
                }
                other => other,
            };
            Ok(Filter::Not(Box::new(node_to_filter(unwrapped)?)))
        }
    }
}

fn generic_err(msg: &str) -> Error {
    Error::Validation(ValidationErrors::single(msg.to_string()))
}

fn map_op(op_str: &str, col: &str) -> Result<Op, Error> {
    Ok(match op_str {
        "$eq" => Op::Eq,
        "$ne" => Op::Ne,
        "$null" => Op::IsNull,
        "$gt" => Op::Gt,
        "$gte" => Op::Gte,
        "$lt" => Op::Lt,
        "$lte" => Op::Lte,
        "$in" => Op::In,
        "$nin" => Op::NotIn,
        "$contains" => Op::Contains,
        "$startsWith" => Op::StartsWith,
        "$endsWith" => Op::EndsWith,
        "$containsi" => Op::ContainsI,
        other => return Err(field_err(col, format!("unknown operator `{other}`"))),
    })
}

fn field_for<'a>(ct: &'a ContentType, col: &str) -> Result<FieldOrSystem<'a>, Error> {
    if is_system_column(col) {
        return Ok(FieldOrSystem::System(system_kind(col)));
    }
    if let Some(f) = ct.fields.iter().find(|f| f.name == col) {
        return Ok(FieldOrSystem::User(f));
    }
    Err(Error::Validation(ValidationErrors::field(
        col,
        "unknown filter field",
    )))
}

enum FieldOrSystem<'a> {
    User(&'a Field),
    System(FieldKind),
}

impl FieldOrSystem<'_> {
    fn kind(&self) -> FieldKind {
        match self {
            FieldOrSystem::User(f) => f.kind,
            FieldOrSystem::System(k) => *k,
        }
    }
}

fn system_kind(col: &str) -> FieldKind {
    // Pull from the central SYSTEM_COLUMNS table so future additions don't
    // need to be mirrored here. Falls back to Text for unknown columns,
    // which is_system_column should never let through.
    SYSTEM_COLUMNS
        .iter()
        .find(|c| c.name == col)
        .map(|c| c.kind)
        .unwrap_or(FieldKind::Text)
}

fn coerce_value(field: FieldOrSystem<'_>, op: Op, col: &str, raw: &str) -> Result<FilterValue, Error> {
    let kind = field.kind();
    match op {
        Op::IsNull => parse_bool(raw)
            .map(FilterValue::Null)
            .map_err(|reason| field_err(col, reason)),
        Op::Eq | Op::Ne => {
            if raw.eq_ignore_ascii_case("null") {
                return Ok(FilterValue::Bound(BoundValue::Null(kind)));
            }
            coerce_bound(kind, col, raw).map(FilterValue::Bound)
        }
        Op::Gt | Op::Gte | Op::Lt | Op::Lte => {
            if raw.eq_ignore_ascii_case("null") {
                return Err(field_err(col, "order operator cannot compare against null"));
            }
            coerce_bound(kind, col, raw).map(FilterValue::Bound)
        }
        Op::Contains | Op::StartsWith | Op::EndsWith | Op::ContainsI => {
            let escaped = escape_like(raw);
            let wrapped = wrap_like(op, escaped);
            Ok(FilterValue::Bound(BoundValue::Str(wrapped)))
        }
        // Set ops are handled directly in `parse`, not here.
        Op::In | Op::NotIn => {
            Err(field_err(col, "internal: set op routed through coerce_value"))
        }
        // Unreachable today: every Op variant above is handled. The wildcard
        // exists because `Op` is `#[non_exhaustive]` so a future variant
        // compiles silently until both `map_op` and this match get updated.
        _ => Err(field_err(col, "unsupported operator")),
    }
}

fn coerce_bound(kind: FieldKind, col: &str, raw: &str) -> Result<BoundValue, Error> {
    let v = match kind {
        FieldKind::String | FieldKind::Text => BoundValue::Str(raw.to_string()),
        FieldKind::Integer => raw
            .parse::<i64>()
            .map(BoundValue::I64)
            .map_err(|_| field_err(col, "expected integer"))?,
        FieldKind::Float => raw
            .parse::<f64>()
            .map(BoundValue::F64)
            .map_err(|_| field_err(col, "expected number"))?,
        FieldKind::Boolean => parse_bool(raw)
            .map(BoundValue::Bool)
            .map_err(|reason| field_err(col, reason))?,
        FieldKind::Datetime => chrono::DateTime::parse_from_rfc3339(raw)
            .map(|t| BoundValue::DateTime(t.with_timezone(&chrono::Utc)))
            .map_err(|_| field_err(col, "expected RFC3339 datetime"))?,
        FieldKind::Uuid => {
            uuid::Uuid::parse_str(raw).map_err(|_| field_err(col, "expected UUID"))?;
            BoundValue::Str(raw.to_string())
        }
        _ => return Err(field_err(col, "unsupported kind for filter")),
    };
    Ok(v)
}

fn parse_bool(raw: &str) -> Result<bool, String> {
    match raw.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("expected `true` or `false`".into()),
    }
}

fn field_err(col: &str, reason: impl Into<String>) -> Error {
    Error::Validation(ValidationErrors::field(col, reason))
}

/// Escape LIKE metacharacters in user input. Order matters: backslash first
/// so we don't double-escape our own substitutions.
fn escape_like(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn wrap_like(op: Op, escaped: String) -> String {
    match op {
        Op::Contains | Op::ContainsI => format!("%{escaped}%"),
        Op::StartsWith => format!("{escaped}%"),
        Op::EndsWith => format!("%{escaped}"),
        // Unreachable: caller filters by op group.
        _ => escaped,
    }
}

#[cfg(test)]
mod tokenize_tests {
    use super::*;

    #[test]
    fn flat_leaf() {
        let segs = tokenize_key("filters[title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn flat_leaf_with_in_index() {
        let segs = tokenize_key("filters[views][$in][0]").unwrap();
        assert_eq!(segs, vec![
            Segment::Name("views".into()),
            Segment::Op("$in".into()),
            Segment::Index(0),
        ]);
    }

    #[test]
    fn or_group_index_then_leaf() {
        let segs = tokenize_key("filters[$or][0][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$or".into()),
            Segment::Index(0),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn not_wraps_leaf() {
        let segs = tokenize_key("filters[$not][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$not".into()),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn nested_or_in_or() {
        let segs = tokenize_key("filters[$or][0][$or][1][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$or".into()),
            Segment::Index(0),
            Segment::Combinator("$or".into()),
            Segment::Index(1),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn missing_filters_prefix_rejected() {
        assert!(tokenize_key("title[$eq]").is_err());
    }

    #[test]
    fn unbalanced_brackets_rejected() {
        assert!(tokenize_key("filters[title][$eq").is_err());
    }
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
            fields: vec![
                Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: true,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "views".into(),
                    kind: FieldKind::Integer,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "published".into(),
                    kind: FieldKind::Boolean,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn leaves(f: Filter) -> Vec<Condition> {
        let Filter::All(xs) = f else { panic!("expected All") };
        xs.into_iter()
            .map(|x| match x {
                Filter::Leaf(c) => c,
                other => panic!("expected Leaf, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn empty_returns_none() {
        let f = parse("", &ct()).unwrap();
        assert!(matches!(f, Filter::None));
    }

    #[test]
    fn ignores_non_filter_params() {
        let f = parse("page=1&pageSize=25&sort=created_at:desc", &ct()).unwrap();
        assert!(matches!(f, Filter::None));
    }

    #[test]
    fn single_eq_string() {
        let f = parse("filters[title][$eq]=hi", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds.len(), 1);
        assert_eq!(conds[0].column, "title");
        assert_eq!(conds[0].op, Op::Eq);
    }

    #[test]
    fn integer_coerces() {
        let f = parse("filters[views][$ne]=7", &ct()).unwrap();
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::I64(n)) => assert_eq!(*n, 7),
            other => panic!("expected I64, got {other:?}"),
        }
    }

    #[test]
    fn bad_integer_rejected() {
        let err = parse("filters[views][$eq]=not-a-number", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn unknown_field_rejected() {
        let err = parse("filters[ghost][$eq]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn unknown_op_rejected() {
        let err = parse("filters[title][$bogus]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn malformed_bracket_rejected() {
        let err = parse("filters[title]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn null_true_and_false() {
        let f = parse("filters[views][$null]=true", &ct()).unwrap();
        let conds = leaves(f);
        assert!(matches!(conds[0].value, FilterValue::Null(true)));

        let f = parse("filters[views][$null]=false", &ct()).unwrap();
        let conds = leaves(f);
        assert!(matches!(conds[0].value, FilterValue::Null(false)));
    }

    #[test]
    fn null_value_invalid() {
        let err = parse("filters[views][$null]=maybe", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn eq_null_rewrites_to_typed_null() {
        let f = parse("filters[views][$eq]=null", &ct()).unwrap();
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Null(k)) => assert_eq!(*k, FieldKind::Integer),
            other => panic!("expected typed Null, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_col_op_rejected() {
        let err = parse(
            "filters[views][$eq]=1&filters[views][$eq]=2",
            &ct(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn same_col_different_ops_allowed() {
        let f = parse("filters[views][$eq]=1&filters[views][$ne]=5", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds.len(), 2);
    }

    #[test]
    fn boolean_case_insensitive() {
        let f = parse("filters[published][$eq]=True", &ct()).unwrap();
        let conds = leaves(f);
        assert!(matches!(conds[0].value, FilterValue::Bound(BoundValue::Bool(true))));
    }

    #[test]
    fn system_column_filterable() {
        let f = parse("filters[id][$null]=false", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds[0].column, "id");
    }

    #[test]
    fn escape_like_handles_metacharacters() {
        assert_eq!(escape_like("foo"), "foo");
        assert_eq!(escape_like("50%"), "50\\%");
        assert_eq!(escape_like("a_b"), "a\\_b");
        assert_eq!(escape_like("a\\b"), "a\\\\b");
        // Backslash-first ordering: input \% becomes \\\% not \\\\%.
        assert_eq!(escape_like("\\%"), "\\\\\\%");
    }

    #[test]
    fn wrap_like_per_op() {
        assert_eq!(wrap_like(Op::Contains, "foo".into()), "%foo%");
        assert_eq!(wrap_like(Op::ContainsI, "foo".into()), "%foo%");
        assert_eq!(wrap_like(Op::StartsWith, "foo".into()), "foo%");
        assert_eq!(wrap_like(Op::EndsWith, "foo".into()), "%foo");
    }

    #[test]
    fn gt_on_string_rejected() {
        let err = parse("filters[title][$gt]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn contains_on_integer_rejected() {
        let err = parse("filters[views][$contains]=7", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn gt_integer_parses() {
        let f = parse("filters[views][$gt]=10", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds[0].op, Op::Gt);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::I64(n)) => assert_eq!(*n, 10),
            other => panic!("expected I64, got {other:?}"),
        }
    }

    #[test]
    fn in_two_values_collects_into_list() {
        let f = parse("filters[views][$in][0]=1&filters[views][$in][1]=2", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds.len(), 1);
        assert_eq!(conds[0].op, Op::In);
        match &conds[0].value {
            FilterValue::List(vs) => {
                assert_eq!(vs.len(), 2);
                assert!(matches!(vs[0], BoundValue::I64(1)));
                assert!(matches!(vs[1], BoundValue::I64(2)));
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn in_missing_index_rejected() {
        let err = parse("filters[views][$in]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn non_set_op_with_index_rejected() {
        let err = parse("filters[views][$eq][0]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_duplicate_index_rejected() {
        let err = parse(
            "filters[views][$in][0]=1&filters[views][$in][0]=2",
            &ct(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_null_entry_rejected() {
        let err = parse("filters[views][$in][0]=null", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_over_cap_rejected() {
        let mut q = String::new();
        for i in 0..=100 {
            if !q.is_empty() {
                q.push('&');
            }
            q.push_str(&format!("filters[views][$in][{i}]={i}"));
        }
        let err = parse(&q, &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn contains_escapes_and_wraps() {
        let f = parse("filters[title][$contains]=50%25", &ct()).unwrap();
        // `%25` URL-decodes to `%`, which then escapes to `\%`, then wraps to `%50\%%`.
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%50\\%%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn starts_with_wraps_one_side() {
        let f = parse("filters[title][$startsWith]=foo", &ct()).unwrap();
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "foo%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn ends_with_wraps_one_side() {
        let f = parse("filters[title][$endsWith]=foo", &ct()).unwrap();
        let conds = leaves(f);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%foo"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn containsi_op_variant() {
        let f = parse("filters[title][$containsi]=FOO", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds[0].op, Op::ContainsI);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%FOO%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn gte_on_datetime_rfc3339() {
        let f = parse("filters[created_at][$gte]=2026-01-01T00:00:00Z", &ct()).unwrap();
        let conds = leaves(f);
        assert_eq!(conds[0].op, Op::Gte);
        assert!(matches!(conds[0].value, FilterValue::Bound(BoundValue::DateTime(_))));
    }

    #[test]
    fn or_two_leaves() {
        let f = parse(
            "filters[$or][0][title][$eq]=foo&filters[$or][1][title][$eq]=bar",
            &ct(),
        )
        .unwrap();
        let Filter::All(xs) = f else { panic!("expected All, got {f:?}") };
        assert_eq!(xs.len(), 1);
        let Filter::Any(ys) = &xs[0] else { panic!("expected Any, got {:?}", xs[0]) };
        assert_eq!(ys.len(), 2);
        for child in ys {
            assert!(matches!(child, Filter::Leaf(_)));
        }
    }

    #[test]
    fn and_two_leaves() {
        let f = parse(
            "filters[$and][0][title][$eq]=foo&filters[$and][1][views][$gt]=1",
            &ct(),
        )
        .unwrap();
        let Filter::All(xs) = f else { panic!() };
        assert_eq!(xs.len(), 1);
        let Filter::All(inner) = &xs[0] else { panic!() };
        assert_eq!(inner.len(), 2);
    }

    #[test]
    fn not_unary_leaf() {
        let f = parse("filters[$not][title][$eq]=foo", &ct()).unwrap();
        let Filter::All(xs) = f else { panic!() };
        let Filter::Not(inner) = &xs[0] else { panic!() };
        assert!(matches!(**inner, Filter::Leaf(_)));
    }

    #[test]
    fn not_wraps_or() {
        let f = parse(
            "filters[$not][$or][0][title][$eq]=foo&filters[$not][$or][1][title][$eq]=bar",
            &ct(),
        )
        .unwrap();
        let Filter::All(xs) = f else { panic!() };
        let Filter::Not(inner) = &xs[0] else { panic!() };
        let Filter::Any(ys) = inner.as_ref() else { panic!() };
        assert_eq!(ys.len(), 2);
    }

    #[test]
    fn mixed_top_level_and_or() {
        let f = parse(
            "filters[published][$eq]=true\
             &filters[$or][0][title][$eq]=foo\
             &filters[$or][1][title][$eq]=bar",
            &ct(),
        )
        .unwrap();
        let Filter::All(xs) = f else { panic!() };
        assert_eq!(xs.len(), 2);
        assert!(matches!(xs[0], Filter::Leaf(_)));
        assert!(matches!(xs[1], Filter::Any(_)));
    }

    #[test]
    fn nested_and_inside_or() {
        let f = parse(
            "filters[$or][0][$and][0][title][$eq]=foo\
             &filters[$or][0][$and][1][views][$gt]=5\
             &filters[$or][1][title][$eq]=bar",
            &ct(),
        )
        .unwrap();
        let Filter::All(xs) = f else { panic!() };
        let Filter::Any(ys) = &xs[0] else { panic!() };
        assert_eq!(ys.len(), 2);
        let Filter::All(zs) = &ys[0] else { panic!() };
        assert_eq!(zs.len(), 2);
        assert!(matches!(ys[1], Filter::Leaf(_)));
    }

    #[test]
    fn not_with_index_rejected() {
        let err = parse("filters[$not][0][title][$eq]=foo", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn or_gap_in_indices_rejected() {
        let err = parse(
            "filters[$or][0][title][$eq]=foo&filters[$or][2][title][$eq]=bar",
            &ct(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn or_duplicate_index_rejected() {
        let err = parse(
            "filters[$or][0][title][$eq]=foo&filters[$or][0][title][$eq]=bar",
            &ct(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn empty_or_rejected() {
        let err = parse("filters[$or]=foo", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_inside_or() {
        let f = parse(
            "filters[$or][0][views][$in][0]=1&filters[$or][0][views][$in][1]=2\
             &filters[$or][1][title][$eq]=foo",
            &ct(),
        )
        .unwrap();
        let Filter::All(xs) = f else { panic!() };
        let Filter::Any(ys) = &xs[0] else { panic!() };
        assert_eq!(ys.len(), 2);
        let Filter::Leaf(c) = &ys[0] else { panic!() };
        assert_eq!(c.op, Op::In);
        let FilterValue::List(vs) = &c.value else { panic!() };
        assert_eq!(vs.len(), 2);
    }

    #[test]
    fn depth_8_allowed() {
        // 8 combinator levels deep: $or > $or > ... > $or (×8) > leaf.
        let mut q = String::new();
        for _ in 0..8 {
            q.push_str("[$or][0]");
        }
        let key = format!("filters{q}[title][$eq]");
        let url = format!("{key}=foo");
        let _ = parse(&url, &ct()).unwrap();
    }

    #[test]
    fn depth_9_rejected() {
        let mut q = String::new();
        for _ in 0..9 {
            q.push_str("[$or][0]");
        }
        let key = format!("filters{q}[title][$eq]");
        let url = format!("{key}=foo");
        let err = parse(&url, &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn leaf_count_100_allowed() {
        let mut parts = Vec::new();
        for i in 0..100 {
            parts.push(format!("filters[$or][{i}][title][$eq]=v{i}"));
        }
        let url = parts.join("&");
        let _ = parse(&url, &ct()).unwrap();
    }

    #[test]
    fn leaf_count_101_rejected() {
        let mut parts = Vec::new();
        for i in 0..101 {
            parts.push(format!("filters[$or][{i}][title][$eq]=v{i}"));
        }
        let url = parts.join("&");
        let err = parse(&url, &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }
}
