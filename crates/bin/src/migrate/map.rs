use rustapi_core::field::FieldKind;

/// Inferred mapping for a single source column.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Mapping {
    /// Use this FieldKind.
    Field(FieldKind),
    /// Column is a FK — target content type name to be resolved interactively.
    Relation,
    /// Skip this column (id, created_at, updated_at, unsupported types, arrays).
    Skip,
}

/// Map a Postgres data type string (from information_schema.columns) to a Mapping.
/// `udt_name` is the user-defined type name (used for enum detection).
/// `is_fk` indicates the column has a foreign key constraint.
#[allow(dead_code)]
pub fn infer(pg_type: &str, udt_name: &str, is_fk: bool) -> Mapping {
    if is_fk {
        return Mapping::Relation;
    }
    match pg_type {
        "text" | "varchar" | "character varying" | "char" | "character" | "bpchar" => {
            Mapping::Field(FieldKind::String)
        }
        "bool" | "boolean" => Mapping::Field(FieldKind::Boolean),
        "int2" | "int4" | "int8" | "integer" | "bigint" | "smallint" | "serial" | "bigserial"
        | "smallserial" => Mapping::Field(FieldKind::Integer),
        "float4" | "float8" | "real" | "double precision" | "numeric" | "decimal" => {
            Mapping::Field(FieldKind::Float)
        }
        "date"
        | "timestamp"
        | "timestamptz"
        | "timestamp without time zone"
        | "timestamp with time zone" => Mapping::Field(FieldKind::Datetime),
        "json" | "jsonb" => Mapping::Field(FieldKind::Json),
        "uuid" => Mapping::Skip,
        "USER-DEFINED" => {
            // Postgres enums show up as USER-DEFINED; udt_name holds the enum type name.
            let _ = udt_name; // enum name used by inspect.rs to fetch values
            Mapping::Field(FieldKind::Enum)
        }
        t if t.starts_with('_') || t.contains("[]") => Mapping::Skip, // arrays
        _ => Mapping::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustapi_core::field::FieldKind;

    #[test]
    fn text_variants_map_to_string() {
        for t in &["text", "varchar", "character varying", "char", "bpchar"] {
            assert_eq!(
                infer(t, "", false),
                Mapping::Field(FieldKind::String),
                "failed for {t}"
            );
        }
    }

    #[test]
    fn bool_maps() {
        assert_eq!(infer("bool", "", false), Mapping::Field(FieldKind::Boolean));
        assert_eq!(
            infer("boolean", "", false),
            Mapping::Field(FieldKind::Boolean)
        );
    }

    #[test]
    fn integer_variants() {
        for t in &[
            "int2",
            "int4",
            "int8",
            "integer",
            "bigint",
            "smallint",
            "serial",
            "bigserial",
        ] {
            assert_eq!(
                infer(t, "", false),
                Mapping::Field(FieldKind::Integer),
                "failed for {t}"
            );
        }
    }

    #[test]
    fn float_variants() {
        for t in &[
            "float4",
            "float8",
            "real",
            "double precision",
            "numeric",
            "decimal",
        ] {
            assert_eq!(
                infer(t, "", false),
                Mapping::Field(FieldKind::Float),
                "failed for {t}"
            );
        }
    }

    #[test]
    fn datetime_variants() {
        for t in &[
            "date",
            "timestamp",
            "timestamptz",
            "timestamp without time zone",
            "timestamp with time zone",
        ] {
            assert_eq!(
                infer(t, "", false),
                Mapping::Field(FieldKind::Datetime),
                "failed for {t}"
            );
        }
    }

    #[test]
    fn json_variants() {
        assert_eq!(infer("json", "", false), Mapping::Field(FieldKind::Json));
        assert_eq!(infer("jsonb", "", false), Mapping::Field(FieldKind::Json));
    }

    #[test]
    fn uuid_skipped() {
        assert_eq!(infer("uuid", "", false), Mapping::Skip);
    }

    #[test]
    fn array_types_skipped() {
        assert_eq!(infer("_text", "", false), Mapping::Skip);
        assert_eq!(infer("_int4", "", false), Mapping::Skip);
    }

    #[test]
    fn unknown_type_skipped() {
        assert_eq!(infer("bytea", "", false), Mapping::Skip);
        assert_eq!(infer("point", "", false), Mapping::Skip);
    }

    #[test]
    fn fk_column_becomes_relation_regardless_of_type() {
        assert_eq!(infer("uuid", "", true), Mapping::Relation);
        assert_eq!(infer("int4", "", true), Mapping::Relation);
    }

    #[test]
    fn user_defined_enum() {
        assert_eq!(
            infer("USER-DEFINED", "my_status", false),
            Mapping::Field(FieldKind::Enum)
        );
    }
}
